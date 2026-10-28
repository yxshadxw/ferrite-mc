//! Detection of JSON fields dropped by the typed model.
//!
//! `serde_ignored` cannot see fields inside untagged enums (this crate has several), so strict
//! mode instead re-serializes the typed value and verifies that every key of the source JSON
//! is still present. Extra keys produced by serialization (defaults, normalized ids) are fine.

use serde_json::Value;

/// Returns the path of the first key present in `original` but missing from `round_trip`.
pub(crate) fn first_dropped_key(original: &Value, round_trip: &Value) -> Option<String> {
    check(original, round_trip, "")
}

fn check(original: &Value, round_trip: &Value, path: &str) -> Option<String> {
    match (original, round_trip) {
        (Value::Object(original), Value::Object(round_trip)) => {
            for (key, value) in original {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match round_trip.get(key) {
                    Some(value_round_trip) => {
                        if let Some(dropped) = check(value, value_round_trip, &child_path) {
                            return Some(dropped);
                        }
                    }
                    None => return Some(child_path),
                }
            }
            None
        }
        (Value::Array(original), Value::Array(round_trip)) => {
            if original.len() != round_trip.len() {
                return Some(format!(
                    "{} (array length {} became {})",
                    display_path(path),
                    original.len(),
                    round_trip.len()
                ));
            }
            for (index, (value, value_round_trip)) in
                original.iter().zip(round_trip.iter()).enumerate()
            {
                let child_path = format!("{path}[{index}]");
                if let Some(dropped) = check(value, value_round_trip, &child_path) {
                    return Some(dropped);
                }
            }
            None
        }
        (Value::Object(_), _) => Some(format!(
            "{} (an object became {})",
            display_path(path),
            kind(round_trip)
        )),
        (Value::Array(_), _) => Some(format!(
            "{} (an array became {})",
            display_path(path),
            kind(round_trip)
        )),
        _ => None,
    }
}

fn display_path(path: &str) -> &str {
    if path.is_empty() { "$" } else { path }
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::first_dropped_key;
    use serde_json::json;

    #[test]
    fn finds_nested_dropped_fields() {
        let original = json!({
            "type": "minecraft:add",
            "left": {"type": "minecraft:noise", "noise": "minecraft:offset", "new_field": 1},
            "right": 0.5
        });
        let round_trip = json!({
            "type": "minecraft:add",
            "left": {"type": "minecraft:noise", "noise": "minecraft:offset"},
            "right": 0.5
        });
        assert_eq!(
            first_dropped_key(&original, &round_trip).as_deref(),
            Some("left.new_field")
        );
    }

    #[test]
    fn accepts_defaults_added_by_serialization() {
        let original = json!({"a": 1.0, "list": [{"x": 2}]});
        let round_trip = json!({"a": 1.0, "list": [{"x": 2, "default": true}], "added": null});
        assert_eq!(first_dropped_key(&original, &round_trip), None);
    }

    #[test]
    fn rejects_changed_shapes_and_array_lengths() {
        assert_eq!(
            first_dropped_key(&json!({"a": {}}), &json!({"a": 1})).as_deref(),
            Some("a (an object became a number)")
        );
        assert_eq!(
            first_dropped_key(&json!({"a": [1, 2]}), &json!({"a": [1]})).as_deref(),
            Some("a (array length 2 became 1)")
        );
    }
}
