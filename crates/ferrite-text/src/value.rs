//! Conversions between raw NBT tags and JSON values for embedded data such as
//! click-event payloads and item component patches.

use crate::{Error, Result};
use ferrite_nbt::{Compound, List, Tag};
use serde_json::{Map, Number, Value};

macro_rules! homogeneous {
    ($tags:expr, $tag:ident, $list:ident) => {{
        let mut values = Vec::with_capacity($tags.len());
        for tag in $tags {
            if let Tag::$tag(value) = tag {
                values.push(value);
            } else {
                return Err(Error::invalid("NBT arrays must be homogeneous"));
            }
        }
        List::$list(values)
    }};
}

/// Converts an NBT tag to its JSON representation. Numeric tags become JSON
/// numbers; the original width is not recoverable.
pub(crate) fn tag_to_json(tag: &Tag) -> Value {
    match tag {
        Tag::Byte(value) => Value::from(*value),
        Tag::Short(value) => Value::from(*value),
        Tag::Int(value) => Value::from(*value),
        Tag::Long(value) => Value::from(*value),
        Tag::Float(value) => Number::from_f64(f64::from(*value)).map_or(Value::Null, Value::Number),
        Tag::Double(value) => Number::from_f64(*value).map_or(Value::Null, Value::Number),
        Tag::ByteArray(values) => Value::Array(values.iter().map(|v| Value::from(*v)).collect()),
        Tag::String(value) => Value::String(value.clone()),
        Tag::List(list) => list_to_json(list),
        Tag::Compound(compound) => compound_to_json(compound),
        Tag::IntArray(values) => Value::Array(values.iter().map(|v| Value::from(*v)).collect()),
        Tag::LongArray(values) => Value::Array(values.iter().map(|v| Value::from(*v)).collect()),
    }
}

fn compound_to_json(compound: &Compound) -> Value {
    let mut map = Map::with_capacity(compound.len());
    for (key, value) in compound {
        map.insert(key.clone(), tag_to_json(value));
    }
    Value::Object(map)
}

fn list_to_json(list: &List) -> Value {
    match list {
        List::End => Value::Array(Vec::new()),
        List::Byte(values) => Value::Array(values.iter().map(|v| Value::from(*v)).collect()),
        List::Short(values) => Value::Array(values.iter().map(|v| Value::from(*v)).collect()),
        List::Int(values) => Value::Array(values.iter().map(|v| Value::from(*v)).collect()),
        List::Long(values) => Value::Array(values.iter().map(|v| Value::from(*v)).collect()),
        List::Float(values) => Value::Array(
            values
                .iter()
                .map(|v| Number::from_f64(f64::from(*v)).map_or(Value::Null, Value::Number))
                .collect(),
        ),
        List::Double(values) => Value::Array(
            values
                .iter()
                .map(|v| Number::from_f64(*v).map_or(Value::Null, Value::Number))
                .collect(),
        ),
        List::ByteArray(values) => Value::Array(
            values
                .iter()
                .map(|v| tag_to_json(&Tag::ByteArray(v.clone())))
                .collect(),
        ),
        List::String(values) => Value::Array(values.iter().cloned().map(Value::String).collect()),
        List::List(values) => Value::Array(values.iter().map(list_to_json).collect()),
        List::Compound(values) => Value::Array(values.iter().map(compound_to_json).collect()),
        List::IntArray(values) => Value::Array(
            values
                .iter()
                .map(|v| tag_to_json(&Tag::IntArray(v.clone())))
                .collect(),
        ),
        List::LongArray(values) => Value::Array(
            values
                .iter()
                .map(|v| tag_to_json(&Tag::LongArray(v.clone())))
                .collect(),
        ),
    }
}

/// Converts a JSON value to an NBT tag.
///
/// JSON numbers are narrowed to the smallest NBT integer type, mirroring
/// vanilla's JSON-to-NBT coercion. Arrays must be homogeneous; mixed arrays
/// are rejected.
pub(crate) fn json_to_tag(value: &Value) -> Result<Tag> {
    match value {
        Value::Null => Err(Error::invalid("null is not a valid NBT value")),
        Value::Bool(value) => Ok(Tag::Byte(i8::from(*value))),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Ok(integer_tag(value))
            } else if let Some(value) = number.as_u64() {
                Ok(Tag::Long(value as i64))
            } else if let Some(value) = number.as_f64() {
                Ok(Tag::Double(value))
            } else {
                Err(Error::invalid("JSON number is not representable in NBT"))
            }
        }
        Value::String(value) => Ok(Tag::String(value.clone())),
        Value::Array(values) => array_to_tag(values),
        Value::Object(object) => {
            let mut compound = Compound::with_capacity(object.len());
            for (key, value) in object {
                compound.insert(key.clone(), json_to_tag(value)?);
            }
            Ok(Tag::Compound(compound))
        }
    }
}

fn integer_tag(value: i64) -> Tag {
    if let Ok(value) = i8::try_from(value) {
        Tag::Byte(value)
    } else if let Ok(value) = i16::try_from(value) {
        Tag::Short(value)
    } else if let Ok(value) = i32::try_from(value) {
        Tag::Int(value)
    } else {
        Tag::Long(value)
    }
}

fn array_to_tag(values: &[Value]) -> Result<Tag> {
    if values.is_empty() {
        return Ok(Tag::List(List::End));
    }
    let tags = values.iter().map(json_to_tag).collect::<Result<Vec<_>>>()?;
    let list = match &tags[0] {
        Tag::Byte(_) => homogeneous!(tags, Byte, Byte),
        Tag::Short(_) => homogeneous!(tags, Short, Short),
        Tag::Int(_) => homogeneous!(tags, Int, Int),
        Tag::Long(_) => homogeneous!(tags, Long, Long),
        Tag::Float(_) => homogeneous!(tags, Float, Float),
        Tag::Double(_) => homogeneous!(tags, Double, Double),
        Tag::ByteArray(_) => homogeneous!(tags, ByteArray, ByteArray),
        Tag::String(_) => homogeneous!(tags, String, String),
        Tag::List(_) => homogeneous!(tags, List, List),
        Tag::Compound(_) => homogeneous!(tags, Compound, Compound),
        Tag::IntArray(_) => homogeneous!(tags, IntArray, IntArray),
        Tag::LongArray(_) => homogeneous!(tags, LongArray, LongArray),
    };
    Ok(Tag::List(list))
}

/// Formats four big-endian UUID integers as a hyphenated UUID string.
pub(crate) fn uuid_from_int_array(values: &[i32]) -> Option<String> {
    if values.len() != 4 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (index, value) in values.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}
