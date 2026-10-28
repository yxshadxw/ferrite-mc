//! The component tree, its content kinds, and flattening helpers.

use crate::style::{ClickEvent, HoverEvent, Style, TextColor};

/// Where an `nbt` component reads its value from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NbtSource {
    /// An entity selector, such as `@p`.
    Entity(String),
    /// Block entity coordinates, such as `~ ~ ~`.
    Block(String),
    /// A command storage resource location, such as `ns:store`.
    Storage(String),
}

impl NbtSource {
    /// The vanilla source name (`entity`, `block` or `storage`).
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Entity(_) => "entity",
            Self::Block(_) => "block",
            Self::Storage(_) => "storage",
        }
    }

    /// The source target string.
    pub fn value(&self) -> &str {
        match self {
            Self::Entity(value) | Self::Block(value) | Self::Storage(value) => value,
        }
    }
}

/// The content of a component: what it displays before styling.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Translatable {
        /// Translation key.
        key: String,
        /// Arguments substituted into the translated string.
        args: Vec<Component>,
        /// Fallback text when the key is missing from the language table.
        fallback: Option<String>,
    },
    Keybind(String),
    Score {
        name: String,
        objective: String,
    },
    Selector {
        selector: String,
        separator: Option<Box<Component>>,
    },
    Nbt {
        path: String,
        source: NbtSource,
        interpret: bool,
        plain: bool,
        separator: Option<Box<Component>>,
    },
}

impl Default for Content {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl Content {
    /// The text shown by this content, before style inheritance.
    ///
    /// Translatable components use their fallback (with arguments
    /// substituted) or their key, since no language table is loaded here.
    /// Score and NBT values need server-side resolution and render as empty
    /// text, matching vanilla's unresolved `getString`.
    fn display_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Translatable {
                key,
                args,
                fallback,
            } => format_translation(fallback.as_deref().unwrap_or(key), args),
            Self::Keybind(keybind) => keybind.clone(),
            Self::Score { .. } => String::new(),
            Self::Selector { selector, .. } => selector.clone(),
            Self::Nbt { .. } => String::new(),
        }
    }
}

/// A Minecraft text component.
///
/// Styles inherit from parent to child; see [`Component::flatten`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Component {
    pub content: Content,
    /// Formatting applied to this component and inherited by its children.
    pub style: Style,
    /// Components displayed after this one, with inherited style.
    pub children: Vec<Component>,
}

/// One flattened piece of text together with its fully resolved style.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub style: Style,
}

impl Component {
    /// A plain text component.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: Content::Text(text.into()),
            ..Self::default()
        }
    }

    /// An empty component.
    pub fn empty() -> Self {
        Self::default()
    }

    /// A translatable component.
    pub fn translatable(key: impl Into<String>) -> Self {
        Self {
            content: Content::Translatable {
                key: key.into(),
                args: Vec::new(),
                fallback: None,
            },
            ..Self::default()
        }
    }

    /// A keybind component, such as `key.jump`.
    pub fn keybind(keybind: impl Into<String>) -> Self {
        Self {
            content: Content::Keybind(keybind.into()),
            ..Self::default()
        }
    }

    /// A scoreboard value component.
    pub fn score(name: impl Into<String>, objective: impl Into<String>) -> Self {
        Self {
            content: Content::Score {
                name: name.into(),
                objective: objective.into(),
            },
            ..Self::default()
        }
    }

    /// An entity-name selector component.
    pub fn selector(selector: impl Into<String>) -> Self {
        Self {
            content: Content::Selector {
                selector: selector.into(),
                separator: None,
            },
            ..Self::default()
        }
    }

    /// An NBT value component.
    pub fn nbt(path: impl Into<String>, source: NbtSource) -> Self {
        Self {
            content: Content::Nbt {
                path: path.into(),
                source,
                interpret: false,
                plain: false,
                separator: None,
            },
            ..Self::default()
        }
    }

    /// Replaces the style entirely.
    #[must_use]
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the text color.
    #[must_use]
    pub fn color(mut self, color: impl Into<TextColor>) -> Self {
        self.style.color = Some(color.into());
        self
    }

    /// Sets the shadow color as a signed ARGB integer.
    #[must_use]
    pub fn shadow_color(mut self, argb: impl Into<i64>) -> Self {
        self.style.shadow_color = Some(argb.into() as i32);
        self
    }

    #[must_use]
    pub fn bold(mut self, value: bool) -> Self {
        self.style.bold = Some(value);
        self
    }

    #[must_use]
    pub fn italic(mut self, value: bool) -> Self {
        self.style.italic = Some(value);
        self
    }

    #[must_use]
    pub fn underlined(mut self, value: bool) -> Self {
        self.style.underlined = Some(value);
        self
    }

    #[must_use]
    pub fn strikethrough(mut self, value: bool) -> Self {
        self.style.strikethrough = Some(value);
        self
    }

    #[must_use]
    pub fn obfuscated(mut self, value: bool) -> Self {
        self.style.obfuscated = Some(value);
        self
    }

    #[must_use]
    pub fn font(mut self, font: impl Into<String>) -> Self {
        self.style.font = Some(font.into());
        self
    }

    #[must_use]
    pub fn insertion(mut self, insertion: impl Into<String>) -> Self {
        self.style.insertion = Some(insertion.into());
        self
    }

    #[must_use]
    pub fn click_event(mut self, event: ClickEvent) -> Self {
        self.style.click_event = Some(event);
        self
    }

    #[must_use]
    pub fn hover_event(mut self, event: HoverEvent) -> Self {
        self.style.hover_event = Some(event);
        self
    }

    /// Appends a child component (the wire `extra` list).
    #[must_use]
    pub fn child(mut self, child: impl Into<Component>) -> Self {
        self.children.push(child.into());
        self
    }

    /// Appends several child components.
    #[must_use]
    pub fn children<I, C>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<Component>,
    {
        self.children.extend(children.into_iter().map(Into::into));
        self
    }

    /// Adds a translation argument to a translatable component.
    ///
    /// No-op for other content kinds.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<Component>) -> Self {
        if let Content::Translatable { args, .. } = &mut self.content {
            args.push(arg.into());
        }
        self
    }

    /// Sets the fallback text of a translatable component.
    ///
    /// No-op for other content kinds.
    #[must_use]
    pub fn fallback(mut self, fallback: impl Into<String>) -> Self {
        if let Content::Translatable { fallback: slot, .. } = &mut self.content {
            *slot = Some(fallback.into());
        }
        self
    }

    /// Sets the separator used by selector and NBT components.
    ///
    /// No-op for other content kinds.
    #[must_use]
    pub fn separator(mut self, separator: impl Into<Component>) -> Self {
        match &mut self.content {
            Content::Selector {
                separator: slot, ..
            }
            | Content::Nbt {
                separator: slot, ..
            } => *slot = Some(Box::new(separator.into())),
            _ => {}
        }
        self
    }

    /// Flattens the tree into styled text runs, resolving style inheritance.
    pub fn flatten(&self) -> Vec<TextRun> {
        let mut runs = Vec::new();
        self.collect_runs(&Style::default(), &mut runs);
        runs
    }

    fn collect_runs(&self, parent: &Style, runs: &mut Vec<TextRun>) {
        let style = self.style.inherit_from(parent);
        let text = self.content.display_text();
        if !text.is_empty() {
            runs.push(TextRun {
                text,
                style: style.clone(),
            });
        }
        for child in &self.children {
            child.collect_runs(&style, runs);
        }
    }

    /// The visible text with all styling stripped and children concatenated.
    pub fn to_plain_text(&self) -> String {
        self.flatten().into_iter().map(|run| run.text).collect()
    }
}

impl From<&str> for Component {
    fn from(value: &str) -> Self {
        Self::text(value)
    }
}

impl From<String> for Component {
    fn from(value: String) -> Self {
        Self::text(value)
    }
}

impl From<&String> for Component {
    fn from(value: &String) -> Self {
        Self::text(value)
    }
}

impl From<Content> for Component {
    fn from(content: Content) -> Self {
        Self {
            content,
            ..Self::default()
        }
    }
}

/// Substitutes `%s`, `%N$s` and `%%` placeholders in a translation string.
fn format_translation(template: &str, args: &[Component]) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::with_capacity(template.len());
    let mut next = 0usize;
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        if i + 1 >= chars.len() {
            out.push('%');
            break;
        }
        if chars[i + 1] == '%' {
            out.push('%');
            i += 2;
            continue;
        }

        let mut j = i + 1;
        let digits_start = j;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        let mut index = None;
        if j > digits_start && j < chars.len() && chars[j] == '$' {
            let digits: String = chars[digits_start..j].iter().collect();
            index = digits.parse::<usize>().ok().map(|n| n.saturating_sub(1));
            j += 1;
        } else {
            j = i + 1;
        }

        if j < chars.len() && chars[j] == 's' {
            let slot = index.unwrap_or_else(|| {
                let slot = next;
                next += 1;
                slot
            });
            if let Some(arg) = args.get(slot) {
                out.push_str(&arg.to_plain_text());
            }
            i = j + 1;
            continue;
        }

        out.push('%');
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NamedColor;

    #[test]
    fn builder_sets_style() {
        let component = Component::text("hi")
            .color(NamedColor::Red)
            .bold(true)
            .italic(false);
        assert_eq!(component.content, Content::Text("hi".into()));
        assert_eq!(
            component.style.color,
            Some(TextColor::Named(NamedColor::Red))
        );
        assert_eq!(component.style.bold, Some(true));
        assert_eq!(component.style.italic, Some(false));
        assert_eq!(component.style.underlined, None);
    }

    #[test]
    fn flatten_resolves_style_inheritance() {
        let component = Component::text("root")
            .color(NamedColor::Red)
            .bold(true)
            .child(Component::text("kid").italic(true))
            .child(Component::text("off").bold(false).underlined(true));

        let runs = component.flatten();
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].text, "root");
        assert_eq!(runs[0].style.color, Some(TextColor::Named(NamedColor::Red)));
        assert_eq!(runs[0].style.bold, Some(true));

        assert_eq!(runs[1].text, "kid");
        assert_eq!(runs[1].style.color, Some(TextColor::Named(NamedColor::Red)));
        assert_eq!(runs[1].style.bold, Some(true));
        assert_eq!(runs[1].style.italic, Some(true));

        assert_eq!(runs[2].style.bold, Some(false));
        assert_eq!(runs[2].style.underlined, Some(true));
        assert_eq!(component.to_plain_text(), "rootkidoff");
    }

    #[test]
    fn translation_arguments_are_substituted() {
        let component = Component::translatable("chat.type.text")
            .fallback("<%s> %s")
            .arg("Steve")
            .arg("hello");
        assert_eq!(component.to_plain_text(), "<Steve> hello");

        let reordered = Component::translatable("k")
            .fallback("%2$s then %1$s and 100%%")
            .arg("one")
            .arg("two");
        assert_eq!(reordered.to_plain_text(), "two then one and 100%");
    }

    #[test]
    fn unresolved_content_falls_back_to_identifiers() {
        assert_eq!(Component::keybind("key.jump").to_plain_text(), "key.jump");
        assert_eq!(Component::selector("@p").to_plain_text(), "@p");
        assert_eq!(Component::score("Steve", "points").to_plain_text(), "");
        assert_eq!(
            Component::nbt("foo", NbtSource::Entity("@p".into())).to_plain_text(),
            ""
        );
    }

    #[test]
    fn children_can_be_added_in_bulk() {
        let component = Component::text("a")
            .children([Component::text("b"), Component::text("c")])
            .child("d");
        assert_eq!(component.children.len(), 3);
        assert_eq!(component.to_plain_text(), "abcd");
    }
}
