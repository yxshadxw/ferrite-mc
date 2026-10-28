//! JSON representation of text components in the 26.x shape.
//!
//! Serialization writes the canonical object form (the `type` discriminator
//! and the redundant `source` field are accepted on input but not emitted).
//! Deserialization also accepts vanilla's shorthands: a bare string, or an
//! array whose first element becomes the root and whose remaining elements
//! become its children.

use crate::style::{ClickEvent, HoverEvent, Style, TextColor};
use crate::value::{json_to_tag, tag_to_json};
use crate::{Component, Content, Error, NbtSource, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

impl Component {
    /// Returns the canonical JSON value for this component.
    pub fn to_json_value(&self) -> Value {
        component_to_json(self)
    }

    /// Builds a component from a JSON value.
    pub fn from_json_value(value: Value) -> Result<Self> {
        component_from_json(&value)
    }

    /// Serializes this component to a JSON string.
    pub fn to_json(&self) -> String {
        self.to_json_value().to_string()
    }

    /// Serializes this component to indented JSON.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.to_json_value())
            .expect("a text component JSON value is always serializable")
    }

    /// Parses a component from a JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

impl Serialize for Component {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_json_value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Component {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        component_from_json(&value).map_err(serde::de::Error::custom)
    }
}

pub(crate) fn component_to_json(component: &Component) -> Value {
    let mut object = Map::new();
    content_to_json(component, &mut object);
    if !component.children.is_empty() {
        object.insert(
            "extra".to_owned(),
            Value::Array(component.children.iter().map(component_to_json).collect()),
        );
    }
    style_to_json(&component.style, &mut object);
    Value::Object(object)
}

pub(crate) fn component_from_json(value: &Value) -> Result<Component> {
    match value {
        Value::String(text) => Ok(Component::text(text.clone())),
        Value::Array(items) => {
            let mut iter = items.iter();
            let first = iter
                .next()
                .ok_or_else(|| Error::invalid("a text component array must not be empty"))?;
            let mut component = component_from_json(first)?;
            for item in iter {
                component.children.push(component_from_json(item)?);
            }
            Ok(component)
        }
        Value::Object(object) => {
            let content = content_from_object(object)?;
            let style = style_from_object(object)?;
            let children = match object.get("extra") {
                None => Vec::new(),
                Some(Value::Array(items)) => items
                    .iter()
                    .map(component_from_json)
                    .collect::<Result<Vec<_>>>()?,
                Some(_) => return Err(Error::invalid("`extra` must be an array")),
            };
            Ok(Component {
                content,
                style,
                children,
            })
        }
        _ => Err(Error::invalid(
            "a text component must be a string, array or object",
        )),
    }
}

fn content_from_object(object: &Map<String, Value>) -> Result<Content> {
    if let Some(value) = object.get("type") {
        let kind = value
            .as_str()
            .ok_or_else(|| Error::invalid("`type` must be a string"))?;
        return content_by_kind(kind, object);
    }

    if object.contains_key("text") {
        text_content(object)
    } else if object.contains_key("translate") {
        translatable_content(object)
    } else if object.contains_key("score") {
        score_content(object)
    } else if object.contains_key("selector") {
        selector_content(object)
    } else if object.contains_key("keybind") {
        keybind_content(object)
    } else if object.contains_key("nbt") {
        nbt_content(object)
    } else {
        Err(Error::invalid(
            "missing content (expected text, translate, score, selector, keybind or nbt)",
        ))
    }
}

fn content_by_kind(kind: &str, object: &Map<String, Value>) -> Result<Content> {
    match kind {
        "text" => text_content(object),
        "translatable" => translatable_content(object),
        "keybind" => keybind_content(object),
        "score" => score_content(object),
        "selector" => selector_content(object),
        "nbt" => nbt_content(object),
        other => Err(Error::invalid(format!(
            "unsupported text component type `{other}`"
        ))),
    }
}

fn text_content(object: &Map<String, Value>) -> Result<Content> {
    Ok(Content::Text(required_string(object, "text")?.to_owned()))
}

fn translatable_content(object: &Map<String, Value>) -> Result<Content> {
    let key = required_string(object, "translate")?.to_owned();
    let fallback = string_field(object, "fallback")?.map(str::to_owned);
    let args = match object.get("with") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(component_from_json)
            .collect::<Result<Vec<_>>>()?,
        Some(_) => return Err(Error::invalid("`with` must be an array")),
    };
    Ok(Content::Translatable {
        key,
        args,
        fallback,
    })
}

fn keybind_content(object: &Map<String, Value>) -> Result<Content> {
    Ok(Content::Keybind(
        required_string(object, "keybind")?.to_owned(),
    ))
}

fn score_content(object: &Map<String, Value>) -> Result<Content> {
    let score = object
        .get("score")
        .ok_or_else(|| Error::invalid("missing `score`"))?;
    let score = object_ref(score, "score")?;
    Ok(Content::Score {
        name: required_string(score, "name")?.to_owned(),
        objective: required_string(score, "objective")?.to_owned(),
    })
}

fn selector_content(object: &Map<String, Value>) -> Result<Content> {
    let selector = required_string(object, "selector")?.to_owned();
    let separator = optional_component(object, "separator")?.map(Box::new);
    Ok(Content::Selector {
        selector,
        separator,
    })
}

fn nbt_content(object: &Map<String, Value>) -> Result<Content> {
    let path = required_string(object, "nbt")?.to_owned();
    let interpret = bool_field(object, "interpret")?.unwrap_or(false);
    let plain = bool_field(object, "plain")?.unwrap_or(false);
    if interpret && plain {
        return Err(Error::invalid(
            "`interpret` and `plain` cannot both be enabled",
        ));
    }
    let separator = optional_component(object, "separator")?.map(Box::new);
    let source = nbt_source(object)?;
    Ok(Content::Nbt {
        path,
        source,
        interpret,
        plain,
        separator,
    })
}

fn nbt_source(object: &Map<String, Value>) -> Result<NbtSource> {
    if let Some(kind) = string_field(object, "source")? {
        return match kind {
            "entity" => Ok(NbtSource::Entity(
                required_string(object, "entity")?.to_owned(),
            )),
            "block" => Ok(NbtSource::Block(
                required_string(object, "block")?.to_owned(),
            )),
            "storage" => Ok(NbtSource::Storage(
                required_string(object, "storage")?.to_owned(),
            )),
            other => Err(Error::invalid(format!(
                "unknown NBT source `{other}` (expected entity, block or storage)"
            ))),
        };
    }

    if object.contains_key("block") {
        Ok(NbtSource::Block(
            required_string(object, "block")?.to_owned(),
        ))
    } else if object.contains_key("entity") {
        Ok(NbtSource::Entity(
            required_string(object, "entity")?.to_owned(),
        ))
    } else if object.contains_key("storage") {
        Ok(NbtSource::Storage(
            required_string(object, "storage")?.to_owned(),
        ))
    } else {
        Err(Error::invalid(
            "an nbt component must specify block, entity or storage",
        ))
    }
}

fn content_to_json(component: &Component, object: &mut Map<String, Value>) {
    match &component.content {
        Content::Text(text) => {
            object.insert("text".to_owned(), Value::String(text.clone()));
        }
        Content::Translatable {
            key,
            args,
            fallback,
        } => {
            object.insert("translate".to_owned(), Value::String(key.clone()));
            if let Some(fallback) = fallback {
                object.insert("fallback".to_owned(), Value::String(fallback.clone()));
            }
            if !args.is_empty() {
                object.insert(
                    "with".to_owned(),
                    Value::Array(args.iter().map(component_to_json).collect()),
                );
            }
        }
        Content::Keybind(keybind) => {
            object.insert("keybind".to_owned(), Value::String(keybind.clone()));
        }
        Content::Score { name, objective } => {
            let mut score = Map::new();
            score.insert("name".to_owned(), Value::String(name.clone()));
            score.insert("objective".to_owned(), Value::String(objective.clone()));
            object.insert("score".to_owned(), Value::Object(score));
        }
        Content::Selector {
            selector,
            separator,
        } => {
            object.insert("selector".to_owned(), Value::String(selector.clone()));
            if let Some(separator) = separator {
                object.insert("separator".to_owned(), component_to_json(separator));
            }
        }
        Content::Nbt {
            path,
            source,
            interpret,
            plain,
            separator,
        } => {
            object.insert("nbt".to_owned(), Value::String(path.clone()));
            if *interpret {
                object.insert("interpret".to_owned(), Value::Bool(true));
            }
            if *plain {
                object.insert("plain".to_owned(), Value::Bool(true));
            }
            if let Some(separator) = separator {
                object.insert("separator".to_owned(), component_to_json(separator));
            }
            object.insert("source".to_owned(), Value::String(source.kind().to_owned()));
            object.insert(
                source.kind().to_owned(),
                Value::String(source.value().to_owned()),
            );
        }
    }
}

fn style_to_json(style: &Style, object: &mut Map<String, Value>) {
    if let Some(color) = style.color {
        object.insert("color".to_owned(), Value::String(color.to_string()));
    }
    if let Some(shadow_color) = style.shadow_color {
        object.insert(
            "shadow_color".to_owned(),
            Value::from(i64::from(shadow_color)),
        );
    }
    if let Some(bold) = style.bold {
        object.insert("bold".to_owned(), Value::Bool(bold));
    }
    if let Some(italic) = style.italic {
        object.insert("italic".to_owned(), Value::Bool(italic));
    }
    if let Some(underlined) = style.underlined {
        object.insert("underlined".to_owned(), Value::Bool(underlined));
    }
    if let Some(strikethrough) = style.strikethrough {
        object.insert("strikethrough".to_owned(), Value::Bool(strikethrough));
    }
    if let Some(obfuscated) = style.obfuscated {
        object.insert("obfuscated".to_owned(), Value::Bool(obfuscated));
    }
    if let Some(click_event) = &style.click_event {
        object.insert("click_event".to_owned(), click_to_json(click_event));
    }
    if let Some(hover_event) = &style.hover_event {
        object.insert("hover_event".to_owned(), hover_to_json(hover_event));
    }
    if let Some(insertion) = &style.insertion {
        object.insert("insertion".to_owned(), Value::String(insertion.clone()));
    }
    if let Some(font) = &style.font {
        object.insert("font".to_owned(), Value::String(font.clone()));
    }
}

fn style_from_object(object: &Map<String, Value>) -> Result<Style> {
    let mut style = Style::new();
    if let Some(value) = object.get("color") {
        let raw = value
            .as_str()
            .ok_or_else(|| Error::invalid("`color` must be a string"))?;
        style.color = Some(
            TextColor::parse(raw)
                .ok_or_else(|| Error::invalid(format!("invalid color `{raw}`")))?,
        );
    }
    if let Some(value) = integer_field(object, "shadow_color")? {
        style.shadow_color = Some(
            i32::try_from(value).map_err(|_| Error::invalid("`shadow_color` is out of range"))?,
        );
    }
    style.bold = bool_field(object, "bold")?;
    style.italic = bool_field(object, "italic")?;
    style.underlined = bool_field(object, "underlined")?;
    style.strikethrough = bool_field(object, "strikethrough")?;
    style.obfuscated = bool_field(object, "obfuscated")?;
    if let Some(value) = object.get("click_event") {
        style.click_event = Some(click_from_json(value)?);
    }
    if let Some(value) = object.get("hover_event") {
        style.hover_event = Some(hover_from_json(value)?);
    }
    style.insertion = string_field(object, "insertion")?.map(str::to_owned);
    style.font = string_field(object, "font")?.map(str::to_owned);
    Ok(style)
}

fn click_to_json(event: &ClickEvent) -> Value {
    let mut object = Map::new();
    object.insert(
        "action".to_owned(),
        Value::String(event.action().to_owned()),
    );
    match event {
        ClickEvent::OpenUrl { url } => {
            object.insert("url".to_owned(), Value::String(url.clone()));
        }
        ClickEvent::RunCommand { command } | ClickEvent::SuggestCommand { command } => {
            object.insert("command".to_owned(), Value::String(command.clone()));
        }
        ClickEvent::ChangePage { page } => {
            object.insert("page".to_owned(), Value::from(*page));
        }
        ClickEvent::CopyToClipboard { value } => {
            object.insert("value".to_owned(), Value::String(value.clone()));
        }
        ClickEvent::ShowDialog { dialog } => {
            object.insert("dialog".to_owned(), Value::String(dialog.clone()));
        }
        ClickEvent::Custom { id, payload } => {
            object.insert("id".to_owned(), Value::String(id.clone()));
            if let Some(payload) = payload {
                object.insert("payload".to_owned(), tag_to_json(payload));
            }
        }
    }
    Value::Object(object)
}

fn click_from_json(value: &Value) -> Result<ClickEvent> {
    let object = object_ref(value, "click_event")?;
    let action = required_string(object, "action")?;
    match action {
        "open_url" => Ok(ClickEvent::open_url(required_string(object, "url")?)),
        "run_command" => Ok(ClickEvent::run_command(required_string(object, "command")?)),
        "suggest_command" => Ok(ClickEvent::suggest_command(required_string(
            object, "command",
        )?)),
        "change_page" => {
            let page =
                integer_field(object, "page")?.ok_or_else(|| Error::invalid("missing `page`"))?;
            let page = i32::try_from(page)
                .ok()
                .filter(|page| *page > 0)
                .ok_or_else(|| Error::invalid("`page` must be a positive integer"))?;
            Ok(ClickEvent::change_page(page))
        }
        "copy_to_clipboard" => Ok(ClickEvent::copy_to_clipboard(required_string(
            object, "value",
        )?)),
        "show_dialog" => Ok(ClickEvent::show_dialog(required_string(object, "dialog")?)),
        "custom" => {
            let id = required_string(object, "id")?.to_owned();
            let payload = object.get("payload").map(json_to_tag).transpose()?;
            Ok(ClickEvent::Custom { id, payload })
        }
        other => Err(Error::invalid(format!(
            "unknown click event action `{other}`"
        ))),
    }
}

fn hover_to_json(event: &HoverEvent) -> Value {
    let mut object = Map::new();
    object.insert(
        "action".to_owned(),
        Value::String(event.action().to_owned()),
    );
    match event {
        HoverEvent::ShowText { value } => {
            object.insert("value".to_owned(), component_to_json(value));
        }
        HoverEvent::ShowItem {
            id,
            count,
            components,
        } => {
            object.insert("id".to_owned(), Value::String(id.clone()));
            if let Some(count) = count {
                object.insert("count".to_owned(), Value::from(*count));
            }
            if let Some(components) = components {
                object.insert("components".to_owned(), tag_to_json(components));
            }
        }
        HoverEvent::ShowEntity { id, uuid, name } => {
            object.insert("id".to_owned(), Value::String(id.clone()));
            object.insert("uuid".to_owned(), Value::String(uuid.clone()));
            if let Some(name) = name {
                object.insert("name".to_owned(), component_to_json(name));
            }
        }
    }
    Value::Object(object)
}

fn hover_from_json(value: &Value) -> Result<HoverEvent> {
    let object = object_ref(value, "hover_event")?;
    let action = required_string(object, "action")?;
    match action {
        "show_text" => {
            let value = object
                .get("value")
                .ok_or_else(|| Error::invalid("missing `value` for show_text"))?;
            Ok(HoverEvent::ShowText {
                value: Box::new(component_from_json(value)?),
            })
        }
        "show_item" => {
            let id = required_string(object, "id")?.to_owned();
            let count = integer_field(object, "count")?
                .map(|value| {
                    i32::try_from(value).map_err(|_| Error::invalid("`count` is out of range"))
                })
                .transpose()?;
            let components = object.get("components").map(json_to_tag).transpose()?;
            Ok(HoverEvent::ShowItem {
                id,
                count,
                components,
            })
        }
        "show_entity" => {
            let id = required_string(object, "id")?.to_owned();
            let uuid = uuid_from_json(
                object
                    .get("uuid")
                    .ok_or_else(|| Error::invalid("missing `uuid` for show_entity"))?,
            )?;
            let name = object
                .get("name")
                .map(component_from_json)
                .transpose()?
                .map(Box::new);
            Ok(HoverEvent::ShowEntity { id, uuid, name })
        }
        other => Err(Error::invalid(format!(
            "unknown hover event action `{other}`"
        ))),
    }
}

fn uuid_from_json(value: &Value) -> Result<String> {
    match value {
        Value::String(uuid) => Ok(uuid.clone()),
        Value::Array(items) => {
            if items.len() != 4 {
                return Err(Error::invalid("a UUID array must contain four integers"));
            }
            let mut values = [0i32; 4];
            for (index, item) in items.iter().enumerate() {
                let value = item
                    .as_i64()
                    .ok_or_else(|| Error::invalid("UUID array entries must be integers"))?;
                values[index] = i32::try_from(value)
                    .map_err(|_| Error::invalid("UUID array entry is out of range"))?;
            }
            crate::value::uuid_from_int_array(&values)
                .ok_or_else(|| Error::invalid("invalid UUID array"))
        }
        _ => Err(Error::invalid(
            "`uuid` must be a string or an array of four integers",
        )),
    }
}

fn optional_component(object: &Map<String, Value>, key: &str) -> Result<Option<Component>> {
    object.get(key).map(component_from_json).transpose()
}

fn object_ref<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| Error::invalid(format!("{what} must be an object")))
}

fn string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(Error::invalid(format!("`{key}` must be a string"))),
    }
}

fn required_string<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    string_field(object, key)?.ok_or_else(|| Error::invalid(format!("missing `{key}`")))
}

fn bool_field(object: &Map<String, Value>, key: &str) -> Result<Option<bool>> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::invalid(format!("`{key}` must be a boolean"))),
    }
}

fn integer_field(object: &Map<String, Value>, key: &str) -> Result<Option<i64>> {
    match object.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| Error::invalid(format!("`{key}` must be an integer"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{ClickEvent, HoverEvent, NamedColor};
    use crate::{Content, NbtSource, TextColor};
    use ferrite_nbt::{Compound, Tag};

    fn round_trip(component: &Component) -> Component {
        let json = component.to_json();
        Component::from_json(&json).unwrap_or_else(|error| panic!("{error}: {json}"))
    }

    #[test]
    fn content_kinds_round_trip() {
        let mut interpreted = Component::nbt("foo", NbtSource::Entity("@p".into()));
        if let Content::Nbt { interpret, .. } = &mut interpreted.content {
            *interpret = true;
        }
        let mut plain = Component::nbt("bar", NbtSource::Storage("ns:store".into()));
        if let Content::Nbt { plain: flag, .. } = &mut plain.content {
            *flag = true;
        }

        let components = [
            Component::text("hello"),
            Component::translatable("chat.type.text")
                .arg(Component::text("Steve"))
                .fallback("fallback"),
            Component::keybind("key.jump"),
            Component::score("Steve", "points"),
            Component::selector("@p").separator(Component::text(", ")),
            Component::nbt("SelectedItem.id", NbtSource::Entity("@p".into())),
            Component::nbt("foo", NbtSource::Block("0 1 2".into())),
            interpreted,
            plain,
        ];

        for component in components {
            assert_eq!(round_trip(&component), component);
        }
    }

    #[test]
    fn full_style_round_trip() {
        let component = Component::text("hi")
            .color(TextColor::rgb(0x00_BEEF))
            .shadow_color(-16_777_216)
            .bold(true)
            .italic(false)
            .underlined(true)
            .strikethrough(false)
            .obfuscated(true)
            .font("minecraft:uniform")
            .insertion("inserted")
            .click_event(ClickEvent::open_url("https://example.com"))
            .hover_event(HoverEvent::show_text(Component::text("tip")));

        let json = component.to_json();
        assert!(json.contains("\"color\":\"#00BEEF\""), "{json}");
        assert!(json.contains("\"shadow_color\":-16777216"), "{json}");
        assert_eq!(round_trip(&component), component);
    }

    #[test]
    fn shorthand_string_and_array() {
        assert_eq!(
            Component::from_json("\"hi\"").unwrap(),
            Component::text("hi")
        );

        let parsed = Component::from_json(r#"["a",{"text":"b","color":"red"},"c"]"#).unwrap();
        let expected = Component::text("a")
            .child(Component::text("b").color(NamedColor::Red))
            .child(Component::text("c"));
        assert_eq!(parsed, expected);
    }

    #[test]
    fn type_field_is_accepted_but_not_emitted() {
        let parsed = Component::from_json(r#"{"type":"text","text":"hi","color":"red"}"#).unwrap();
        assert_eq!(parsed, Component::text("hi").color(NamedColor::Red));
        assert!(!parsed.to_json().contains("\"type\""));

        let parsed = Component::from_json(
            r#"{"type":"translatable","translate":"k","with":["a"],"fallback":"fb"}"#,
        )
        .unwrap();
        assert_eq!(
            parsed,
            Component::translatable("k")
                .arg(Component::text("a"))
                .fallback("fb")
        );
    }

    #[test]
    fn legacy_object_without_type_parses() {
        let parsed = Component::from_json(r#"{"text":"a","extra":["b","c"]}"#).unwrap();
        assert_eq!(
            parsed,
            Component::text("a")
                .child(Component::text("b"))
                .child(Component::text("c"))
        );
    }

    #[test]
    fn click_events_round_trip() {
        let events = [
            ClickEvent::open_url("https://example.com"),
            ClickEvent::run_command("/spawn"),
            ClickEvent::suggest_command("/spawn"),
            ClickEvent::change_page(3),
            ClickEvent::copy_to_clipboard("copied"),
            ClickEvent::show_dialog("minecraft:server_links"),
            ClickEvent::custom("mypack:action").with_payload(Tag::String("data".into())),
        ];
        for event in events {
            let component = Component::text("x").click_event(event);
            assert_eq!(round_trip(&component), component);
        }
    }

    #[test]
    fn hover_events_round_trip() {
        let mut patch = Compound::new();
        patch.insert(
            "minecraft:custom_name".into(),
            Tag::String("renamed".into()),
        );

        let events = [
            HoverEvent::show_text(Component::text("tip").color(NamedColor::Gold)),
            HoverEvent::show_item("minecraft:stone")
                .count(2)
                .components(Tag::Compound(patch)),
            HoverEvent::show_entity("minecraft:pig", "00000000-0000-0000-0000-000000000001")
                .name(Component::text("piggy")),
        ];
        for event in events {
            let component = Component::text("x").hover_event(event);
            assert_eq!(round_trip(&component), component);
        }
    }

    #[test]
    fn nbt_source_is_inferred() {
        let parsed = Component::from_json(r#"{"nbt":"foo","entity":"@p"}"#).unwrap();
        assert_eq!(
            parsed,
            Component::nbt("foo", NbtSource::Entity("@p".into()))
        );

        let parsed =
            Component::from_json(r#"{"nbt":"foo","source":"storage","storage":"ns:id"}"#).unwrap();
        assert_eq!(
            parsed,
            Component::nbt("foo", NbtSource::Storage("ns:id".into()))
        );
    }

    #[test]
    fn invalid_components_are_rejected() {
        for json in [
            "{}",
            "[]",
            "null",
            r#"{"type":"object"}"#,
            r#"{"text":"x","bold":1}"#,
            r#"{"text":"x","extra":"nope"}"#,
            r#"{"nbt":"foo","interpret":true,"plain":true,"entity":"@p"}"#,
            r#"{"nbt":"foo"}"#,
            r#"{"text":"x","click_event":{"action":"bogus"}}"#,
        ] {
            assert!(Component::from_json(json).is_err(), "accepted {json}");
        }
    }
}
