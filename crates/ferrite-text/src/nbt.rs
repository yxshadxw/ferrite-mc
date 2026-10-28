//! NBT representation of text components, including the network form.

use crate::style::{ClickEvent, HoverEvent, Style, TextColor};
use crate::value::uuid_from_int_array;
use crate::{Component, Content, Error, NbtSource, Result};
use ferrite_nbt::{Compound, List, Tag};

impl Component {
    /// Encodes this component as an NBT tag.
    ///
    /// A text component with no style and no children collapses to a
    /// `TAG_String`, matching vanilla's network representation. Every other
    /// component becomes a `TAG_Compound`.
    pub fn to_nbt(&self) -> Tag {
        if let Some(text) = self.collapsed_text() {
            Tag::String(text.to_owned())
        } else {
            Tag::Compound(self.to_compound())
        }
    }

    /// Decodes a component from an NBT tag.
    pub fn from_nbt(tag: &Tag) -> Result<Self> {
        match tag {
            Tag::String(text) => Ok(Self::text(text.clone())),
            Tag::Compound(compound) => Self::from_compound(compound),
            _ => Err(Error::invalid(
                "a text component must be an NBT string or compound",
            )),
        }
    }

    /// Appends this component as nameless-root network NBT.
    ///
    /// # Panics
    ///
    /// Panics if the component exceeds NBT limits, for example a single string
    /// longer than 65,535 modified-UTF-8 bytes. Use
    /// [`Component::try_encode_network`] to handle that case.
    pub fn encode_network(&self, out: &mut Vec<u8>) {
        self.try_encode_network(out)
            .expect("text component exceeds network NBT limits");
    }

    /// Fallible form of [`Component::encode_network`].
    pub fn try_encode_network(&self, out: &mut Vec<u8>) -> Result<()> {
        let bytes = ferrite_nbt::binary::write_network(&self.to_nbt())?;
        out.extend_from_slice(&bytes);
        Ok(())
    }

    /// Decodes a component from nameless-root network NBT.
    pub fn from_network(bytes: &[u8]) -> Result<Self> {
        let tag = ferrite_nbt::binary::read_network(bytes)?;
        Self::from_nbt(&tag)
    }

    fn collapsed_text(&self) -> Option<&str> {
        match (
            &self.content,
            self.style.is_empty(),
            self.children.is_empty(),
        ) {
            (Content::Text(text), true, true) => Some(text),
            _ => None,
        }
    }

    fn to_compound(&self) -> Compound {
        let mut compound = Compound::new();
        content_to_compound(&self.content, &mut compound);
        if !self.children.is_empty() {
            compound.insert(
                "extra".to_owned(),
                Tag::List(component_list(&self.children)),
            );
        }
        style_to_compound(&self.style, &mut compound);
        compound
    }

    fn from_compound(compound: &Compound) -> Result<Self> {
        let content = content_from_compound(compound)?;
        let style = style_from_compound(compound)?;
        let children = match compound.get("extra") {
            None => Vec::new(),
            Some(Tag::List(list)) => components_from_list(list)?,
            Some(_) => return Err(Error::invalid("`extra` must be an NBT list")),
        };
        Ok(Self {
            content,
            style,
            children,
        })
    }
}

fn component_list(components: &[Component]) -> List {
    if components
        .iter()
        .all(|component| component.collapsed_text().is_some())
    {
        List::String(
            components
                .iter()
                .map(|component| {
                    component
                        .collapsed_text()
                        .expect("checked by all()")
                        .to_owned()
                })
                .collect(),
        )
    } else {
        List::Compound(components.iter().map(Component::to_compound).collect())
    }
}

fn content_to_compound(content: &Content, compound: &mut Compound) {
    match content {
        Content::Text(text) => {
            compound.insert("text".to_owned(), Tag::String(text.clone()));
        }
        Content::Translatable {
            key,
            args,
            fallback,
        } => {
            compound.insert("translate".to_owned(), Tag::String(key.clone()));
            if let Some(fallback) = fallback {
                compound.insert("fallback".to_owned(), Tag::String(fallback.clone()));
            }
            if !args.is_empty() {
                compound.insert("with".to_owned(), Tag::List(component_list(args)));
            }
        }
        Content::Keybind(keybind) => {
            compound.insert("keybind".to_owned(), Tag::String(keybind.clone()));
        }
        Content::Score { name, objective } => {
            let mut score = Compound::new();
            score.insert("name".to_owned(), Tag::String(name.clone()));
            score.insert("objective".to_owned(), Tag::String(objective.clone()));
            compound.insert("score".to_owned(), Tag::Compound(score));
        }
        Content::Selector {
            selector,
            separator,
        } => {
            compound.insert("selector".to_owned(), Tag::String(selector.clone()));
            if let Some(separator) = separator {
                compound.insert("separator".to_owned(), separator.to_nbt());
            }
        }
        Content::Nbt {
            path,
            source,
            interpret,
            plain,
            separator,
        } => {
            compound.insert("nbt".to_owned(), Tag::String(path.clone()));
            if *interpret {
                compound.insert("interpret".to_owned(), Tag::Byte(1));
            }
            if *plain {
                compound.insert("plain".to_owned(), Tag::Byte(1));
            }
            if let Some(separator) = separator {
                compound.insert("separator".to_owned(), separator.to_nbt());
            }
            compound.insert("source".to_owned(), Tag::String(source.kind().to_owned()));
            compound.insert(
                source.kind().to_owned(),
                Tag::String(source.value().to_owned()),
            );
        }
    }
}

fn style_to_compound(style: &Style, compound: &mut Compound) {
    if let Some(color) = style.color {
        compound.insert("color".to_owned(), Tag::String(color.to_string()));
    }
    if let Some(shadow_color) = style.shadow_color {
        compound.insert("shadow_color".to_owned(), Tag::Int(shadow_color));
    }
    if let Some(bold) = style.bold {
        compound.insert("bold".to_owned(), Tag::Byte(i8::from(bold)));
    }
    if let Some(italic) = style.italic {
        compound.insert("italic".to_owned(), Tag::Byte(i8::from(italic)));
    }
    if let Some(underlined) = style.underlined {
        compound.insert("underlined".to_owned(), Tag::Byte(i8::from(underlined)));
    }
    if let Some(strikethrough) = style.strikethrough {
        compound.insert(
            "strikethrough".to_owned(),
            Tag::Byte(i8::from(strikethrough)),
        );
    }
    if let Some(obfuscated) = style.obfuscated {
        compound.insert("obfuscated".to_owned(), Tag::Byte(i8::from(obfuscated)));
    }
    if let Some(click_event) = &style.click_event {
        compound.insert("click_event".to_owned(), click_to_tag(click_event));
    }
    if let Some(hover_event) = &style.hover_event {
        compound.insert("hover_event".to_owned(), hover_to_tag(hover_event));
    }
    if let Some(insertion) = &style.insertion {
        compound.insert("insertion".to_owned(), Tag::String(insertion.clone()));
    }
    if let Some(font) = &style.font {
        compound.insert("font".to_owned(), Tag::String(font.clone()));
    }
}

fn content_from_compound(compound: &Compound) -> Result<Content> {
    if let Some(tag) = compound.get("type") {
        let kind = match tag {
            Tag::String(kind) => kind.as_str(),
            _ => return Err(Error::invalid("`type` must be an NBT string")),
        };
        return content_by_kind(kind, compound);
    }

    if compound.contains_key("text") {
        text_content(compound)
    } else if compound.contains_key("translate") {
        translatable_content(compound)
    } else if compound.contains_key("score") {
        score_content(compound)
    } else if compound.contains_key("selector") {
        selector_content(compound)
    } else if compound.contains_key("keybind") {
        keybind_content(compound)
    } else if compound.contains_key("nbt") {
        nbt_content(compound)
    } else {
        Err(Error::invalid(
            "missing content (expected text, translate, score, selector, keybind or nbt)",
        ))
    }
}

fn content_by_kind(kind: &str, compound: &Compound) -> Result<Content> {
    match kind {
        "text" => text_content(compound),
        "translatable" => translatable_content(compound),
        "keybind" => keybind_content(compound),
        "score" => score_content(compound),
        "selector" => selector_content(compound),
        "nbt" => nbt_content(compound),
        other => Err(Error::invalid(format!(
            "unsupported text component type `{other}`"
        ))),
    }
}

fn text_content(compound: &Compound) -> Result<Content> {
    Ok(Content::Text(required_string(compound, "text")?.to_owned()))
}

fn translatable_content(compound: &Compound) -> Result<Content> {
    let key = required_string(compound, "translate")?.to_owned();
    let fallback = string_field(compound, "fallback")?.map(str::to_owned);
    let args = match compound.get("with") {
        None => Vec::new(),
        Some(Tag::List(list)) => components_from_list(list)?,
        Some(_) => return Err(Error::invalid("`with` must be an NBT list")),
    };
    Ok(Content::Translatable {
        key,
        args,
        fallback,
    })
}

fn keybind_content(compound: &Compound) -> Result<Content> {
    Ok(Content::Keybind(
        required_string(compound, "keybind")?.to_owned(),
    ))
}

fn score_content(compound: &Compound) -> Result<Content> {
    let score = match compound.get("score") {
        Some(Tag::Compound(score)) => score,
        Some(_) => return Err(Error::invalid("`score` must be an NBT compound")),
        None => return Err(Error::invalid("missing `score`")),
    };
    Ok(Content::Score {
        name: required_string(score, "name")?.to_owned(),
        objective: required_string(score, "objective")?.to_owned(),
    })
}

fn selector_content(compound: &Compound) -> Result<Content> {
    let selector = required_string(compound, "selector")?.to_owned();
    let separator = match compound.get("separator") {
        None => None,
        Some(tag) => Some(Box::new(Component::from_nbt(tag)?)),
    };
    Ok(Content::Selector {
        selector,
        separator,
    })
}

fn nbt_content(compound: &Compound) -> Result<Content> {
    let path = required_string(compound, "nbt")?.to_owned();
    let interpret = bool_field(compound, "interpret")?.unwrap_or(false);
    let plain = bool_field(compound, "plain")?.unwrap_or(false);
    if interpret && plain {
        return Err(Error::invalid(
            "`interpret` and `plain` cannot both be enabled",
        ));
    }
    let separator = match compound.get("separator") {
        None => None,
        Some(tag) => Some(Box::new(Component::from_nbt(tag)?)),
    };
    let source = nbt_source(compound)?;
    Ok(Content::Nbt {
        path,
        source,
        interpret,
        plain,
        separator,
    })
}

fn nbt_source(compound: &Compound) -> Result<NbtSource> {
    if let Some(kind) = string_field(compound, "source")? {
        return match kind {
            "entity" => Ok(NbtSource::Entity(
                required_string(compound, "entity")?.to_owned(),
            )),
            "block" => Ok(NbtSource::Block(
                required_string(compound, "block")?.to_owned(),
            )),
            "storage" => Ok(NbtSource::Storage(
                required_string(compound, "storage")?.to_owned(),
            )),
            other => Err(Error::invalid(format!(
                "unknown NBT source `{other}` (expected entity, block or storage)"
            ))),
        };
    }

    if compound.contains_key("block") {
        Ok(NbtSource::Block(
            required_string(compound, "block")?.to_owned(),
        ))
    } else if compound.contains_key("entity") {
        Ok(NbtSource::Entity(
            required_string(compound, "entity")?.to_owned(),
        ))
    } else if compound.contains_key("storage") {
        Ok(NbtSource::Storage(
            required_string(compound, "storage")?.to_owned(),
        ))
    } else {
        Err(Error::invalid(
            "an nbt component must specify block, entity or storage",
        ))
    }
}

fn style_from_compound(compound: &Compound) -> Result<Style> {
    let mut style = Style::new();
    if let Some(raw) = string_field(compound, "color")? {
        style.color = Some(
            TextColor::parse(raw)
                .ok_or_else(|| Error::invalid(format!("invalid color `{raw}`")))?,
        );
    }
    if let Some(Tag::Int(shadow_color)) = compound.get("shadow_color") {
        style.shadow_color = Some(*shadow_color);
    } else if compound.contains_key("shadow_color") {
        return Err(Error::invalid("`shadow_color` must be an NBT int"));
    }
    style.bold = bool_field(compound, "bold")?;
    style.italic = bool_field(compound, "italic")?;
    style.underlined = bool_field(compound, "underlined")?;
    style.strikethrough = bool_field(compound, "strikethrough")?;
    style.obfuscated = bool_field(compound, "obfuscated")?;
    if let Some(tag) = compound.get("click_event") {
        let event = match tag {
            Tag::Compound(event) => event,
            _ => return Err(Error::invalid("`click_event` must be an NBT compound")),
        };
        style.click_event = Some(click_from_tag(event)?);
    }
    if let Some(tag) = compound.get("hover_event") {
        let event = match tag {
            Tag::Compound(event) => event,
            _ => return Err(Error::invalid("`hover_event` must be an NBT compound")),
        };
        style.hover_event = Some(hover_from_tag(event)?);
    }
    style.insertion = string_field(compound, "insertion")?.map(str::to_owned);
    style.font = string_field(compound, "font")?.map(str::to_owned);
    Ok(style)
}

fn click_to_tag(event: &ClickEvent) -> Tag {
    let mut compound = Compound::new();
    compound.insert("action".to_owned(), Tag::String(event.action().to_owned()));
    match event {
        ClickEvent::OpenUrl { url } => {
            compound.insert("url".to_owned(), Tag::String(url.clone()));
        }
        ClickEvent::RunCommand { command } | ClickEvent::SuggestCommand { command } => {
            compound.insert("command".to_owned(), Tag::String(command.clone()));
        }
        ClickEvent::ChangePage { page } => {
            compound.insert("page".to_owned(), Tag::Int(*page));
        }
        ClickEvent::CopyToClipboard { value } => {
            compound.insert("value".to_owned(), Tag::String(value.clone()));
        }
        ClickEvent::ShowDialog { dialog } => {
            compound.insert("dialog".to_owned(), Tag::String(dialog.clone()));
        }
        ClickEvent::Custom { id, payload } => {
            compound.insert("id".to_owned(), Tag::String(id.clone()));
            if let Some(payload) = payload {
                compound.insert("payload".to_owned(), payload.clone());
            }
        }
    }
    Tag::Compound(compound)
}

fn click_from_tag(compound: &Compound) -> Result<ClickEvent> {
    let action = required_string(compound, "action")?;
    match action {
        "open_url" => Ok(ClickEvent::open_url(required_string(compound, "url")?)),
        "run_command" => Ok(ClickEvent::run_command(required_string(
            compound, "command",
        )?)),
        "suggest_command" => Ok(ClickEvent::suggest_command(required_string(
            compound, "command",
        )?)),
        "change_page" => {
            let page = match compound.get("page") {
                Some(Tag::Int(page)) => *page,
                Some(_) => return Err(Error::invalid("`page` must be an NBT int")),
                None => return Err(Error::invalid("missing `page`")),
            };
            if page <= 0 {
                return Err(Error::invalid("`page` must be a positive integer"));
            }
            Ok(ClickEvent::change_page(page))
        }
        "copy_to_clipboard" => Ok(ClickEvent::copy_to_clipboard(required_string(
            compound, "value",
        )?)),
        "show_dialog" => Ok(ClickEvent::show_dialog(required_string(
            compound, "dialog",
        )?)),
        "custom" => {
            let id = required_string(compound, "id")?.to_owned();
            Ok(ClickEvent::Custom {
                id,
                payload: compound.get("payload").cloned(),
            })
        }
        other => Err(Error::invalid(format!(
            "unknown click event action `{other}`"
        ))),
    }
}

fn hover_to_tag(event: &HoverEvent) -> Tag {
    let mut compound = Compound::new();
    compound.insert("action".to_owned(), Tag::String(event.action().to_owned()));
    match event {
        HoverEvent::ShowText { value } => {
            compound.insert("value".to_owned(), value.to_nbt());
        }
        HoverEvent::ShowItem {
            id,
            count,
            components,
        } => {
            compound.insert("id".to_owned(), Tag::String(id.clone()));
            if let Some(count) = count {
                compound.insert("count".to_owned(), Tag::Int(*count));
            }
            if let Some(components) = components {
                compound.insert("components".to_owned(), components.clone());
            }
        }
        HoverEvent::ShowEntity { id, uuid, name } => {
            compound.insert("id".to_owned(), Tag::String(id.clone()));
            compound.insert("uuid".to_owned(), Tag::String(uuid.clone()));
            if let Some(name) = name {
                compound.insert("name".to_owned(), name.to_nbt());
            }
        }
    }
    Tag::Compound(compound)
}

fn hover_from_tag(compound: &Compound) -> Result<HoverEvent> {
    let action = required_string(compound, "action")?;
    match action {
        "show_text" => {
            let value = compound
                .get("value")
                .ok_or_else(|| Error::invalid("missing `value` for show_text"))?;
            Ok(HoverEvent::ShowText {
                value: Box::new(Component::from_nbt(value)?),
            })
        }
        "show_item" => {
            let id = required_string(compound, "id")?.to_owned();
            let count = match compound.get("count") {
                Some(Tag::Int(count)) => Some(*count),
                Some(_) => return Err(Error::invalid("`count` must be an NBT int")),
                None => None,
            };
            let components = compound.get("components").cloned();
            Ok(HoverEvent::ShowItem {
                id,
                count,
                components,
            })
        }
        "show_entity" => {
            let id = required_string(compound, "id")?.to_owned();
            let uuid = match compound.get("uuid") {
                Some(Tag::String(uuid)) => uuid.clone(),
                Some(Tag::IntArray(values)) => uuid_from_int_array(values)
                    .ok_or_else(|| Error::invalid("invalid UUID array"))?,
                Some(_) => {
                    return Err(Error::invalid("`uuid` must be an NBT string or int array"));
                }
                None => return Err(Error::invalid("missing `uuid` for show_entity")),
            };
            let name = match compound.get("name") {
                None => None,
                Some(tag) => Some(Box::new(Component::from_nbt(tag)?)),
            };
            Ok(HoverEvent::ShowEntity { id, uuid, name })
        }
        other => Err(Error::invalid(format!(
            "unknown hover event action `{other}`"
        ))),
    }
}

fn components_from_list(list: &List) -> Result<Vec<Component>> {
    match list {
        List::End => Ok(Vec::new()),
        List::String(values) => Ok(values.iter().map(Component::text).collect()),
        List::Compound(values) => values.iter().map(Component::from_compound).collect(),
        _ => Err(Error::invalid(
            "a component list must contain strings or compounds",
        )),
    }
}

fn string_field<'a>(compound: &'a Compound, key: &str) -> Result<Option<&'a str>> {
    match compound.get(key) {
        None => Ok(None),
        Some(Tag::String(value)) => Ok(Some(value)),
        Some(_) => Err(Error::invalid(format!("`{key}` must be an NBT string"))),
    }
}

fn required_string<'a>(compound: &'a Compound, key: &str) -> Result<&'a str> {
    string_field(compound, key)?.ok_or_else(|| Error::invalid(format!("missing `{key}`")))
}

fn bool_field(compound: &Compound, key: &str) -> Result<Option<bool>> {
    match compound.get(key) {
        None => Ok(None),
        Some(Tag::Byte(value)) => Ok(Some(*value != 0)),
        Some(_) => Err(Error::invalid(format!("`{key}` must be an NBT byte"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{ClickEvent, HoverEvent, NamedColor};

    fn round_trip(component: &Component) -> Component {
        let tag = component.to_nbt();
        Component::from_nbt(&tag).unwrap_or_else(|error| panic!("{error}: {tag:?}"))
    }

    #[test]
    fn plain_text_collapses_to_a_string_tag() {
        assert_eq!(
            Component::text("hello").to_nbt(),
            Tag::String("hello".into())
        );
        assert_eq!(Component::empty().to_nbt(), Tag::String(String::new()));
        assert_eq!(Component::text("hello").bold(true).to_nbt().id(), 10);
    }

    #[test]
    fn content_kinds_round_trip() {
        let mut interpreted = Component::nbt("foo", NbtSource::Entity("@p".into()));
        if let Content::Nbt { interpret, .. } = &mut interpreted.content {
            *interpret = true;
        }
        let mut plain = Component::nbt("bar", NbtSource::Block("0 1 2".into()));
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
            Component::nbt("foo", NbtSource::Storage("ns:id".into())),
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
            .color(NamedColor::Gold)
            .shadow_color(0x8000_0000u32)
            .bold(false)
            .italic(true)
            .underlined(false)
            .strikethrough(true)
            .obfuscated(false)
            .font("minecraft:uniform")
            .insertion("inserted")
            .click_event(ClickEvent::change_page(4))
            .hover_event(
                HoverEvent::show_item("minecraft:stone")
                    .count(3)
                    .components(Tag::String("raw".into())),
            );
        assert_eq!(round_trip(&component), component);
    }

    #[test]
    fn click_and_hover_events_round_trip() {
        let clicks = [
            ClickEvent::open_url("https://example.com"),
            ClickEvent::run_command("/spawn"),
            ClickEvent::suggest_command("/spawn"),
            ClickEvent::change_page(3),
            ClickEvent::copy_to_clipboard("copied"),
            ClickEvent::show_dialog("minecraft:server_links"),
            ClickEvent::custom("mypack:action").with_payload(Tag::Int(7)),
        ];
        for event in clicks {
            let component = Component::text("x").click_event(event);
            assert_eq!(round_trip(&component), component);
        }

        let hovers = [
            HoverEvent::show_text(Component::text("tip").bold(true)),
            HoverEvent::show_item("minecraft:stone").count(2),
            HoverEvent::show_entity("minecraft:pig", "00000000-0000-0000-0000-000000000001")
                .name(Component::text("piggy")),
        ];
        for event in hovers {
            let component = Component::text("x").hover_event(event);
            assert_eq!(round_trip(&component), component);
        }
    }

    #[test]
    fn children_use_string_lists_when_possible() {
        let simple = Component::text("a").child(Component::text("b"));
        let Tag::Compound(compound) = simple.to_nbt() else {
            panic!("expected compound");
        };
        assert_eq!(
            compound.get("extra"),
            Some(&Tag::List(List::String(vec!["b".into()])))
        );

        let mixed = Component::text("a").child(Component::text("b").bold(true));
        let Tag::Compound(compound) = mixed.to_nbt() else {
            panic!("expected compound");
        };
        assert!(matches!(
            compound.get("extra"),
            Some(Tag::List(List::Compound(_)))
        ));
    }

    #[test]
    fn network_encoding_and_decoding() {
        let mut out = Vec::new();
        Component::text("hello").encode_network(&mut out);
        assert_eq!(out, vec![0x08, 0x00, 0x05, b'h', b'e', b'l', b'l', b'o']);
        assert_eq!(
            Component::from_network(&out).unwrap(),
            Component::text("hello")
        );

        let styled = Component::text("hi")
            .color(NamedColor::Red)
            .click_event(ClickEvent::open_url("https://example.com"));
        let mut out = Vec::new();
        styled.try_encode_network(&mut out).unwrap();
        assert_eq!(Component::from_network(&out).unwrap(), styled);
    }

    #[test]
    fn type_field_and_legacy_fields_are_accepted() {
        let mut compound = Compound::new();
        compound.insert("type".into(), Tag::String("text".into()));
        compound.insert("text".into(), Tag::String("hi".into()));
        compound.insert("color".into(), Tag::String("red".into()));
        assert_eq!(
            Component::from_nbt(&Tag::Compound(compound)).unwrap(),
            Component::text("hi").color(NamedColor::Red)
        );
    }

    #[test]
    fn show_entity_accepts_int_array_uuids() {
        let mut hover = Compound::new();
        hover.insert("action".into(), Tag::String("show_entity".into()));
        hover.insert("id".into(), Tag::String("minecraft:pig".into()));
        hover.insert("uuid".into(), Tag::IntArray(vec![0, 0, 0, 1]));

        let mut root = Compound::new();
        root.insert("text".into(), Tag::String("x".into()));
        root.insert("hover_event".into(), Tag::Compound(hover));

        let parsed = Component::from_nbt(&Tag::Compound(root)).unwrap();
        assert_eq!(
            parsed.style.hover_event,
            Some(HoverEvent::ShowEntity {
                id: "minecraft:pig".into(),
                uuid: "00000000-0000-0000-0000-000000000001".into(),
                name: None,
            })
        );
    }

    #[test]
    fn invalid_nbt_is_rejected() {
        assert!(Component::from_nbt(&Tag::Int(1)).is_err());
        assert!(Component::from_nbt(&Tag::Compound(Compound::new())).is_err());

        let mut compound = Compound::new();
        compound.insert("text".into(), Tag::String("x".into()));
        compound.insert("extra".into(), Tag::String("nope".into()));
        assert!(Component::from_nbt(&Tag::Compound(compound)).is_err());
    }
}
