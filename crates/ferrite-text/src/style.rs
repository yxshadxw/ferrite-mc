//! Colors, styles and click/hover events for text components.

use crate::Component;
use ferrite_nbt::Tag;
use std::fmt;
use std::str::FromStr;

/// One of the sixteen named vanilla chat colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedColor {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
}

impl NamedColor {
    /// All named colors in vanilla's legacy color-code order.
    pub const ALL: [Self; 16] = [
        Self::Black,
        Self::DarkBlue,
        Self::DarkGreen,
        Self::DarkAqua,
        Self::DarkRed,
        Self::DarkPurple,
        Self::Gold,
        Self::Gray,
        Self::DarkGray,
        Self::Blue,
        Self::Green,
        Self::Aqua,
        Self::Red,
        Self::LightPurple,
        Self::Yellow,
        Self::White,
    ];

    /// The RGB value vanilla uses for this color.
    pub const fn rgb(self) -> u32 {
        match self {
            Self::Black => 0x00_00_00,
            Self::DarkBlue => 0x00_00_AA,
            Self::DarkGreen => 0x00_AA_00,
            Self::DarkAqua => 0x00_AA_AA,
            Self::DarkRed => 0xAA_00_00,
            Self::DarkPurple => 0xAA_00_AA,
            Self::Gold => 0xFF_AA_00,
            Self::Gray => 0xAA_AA_AA,
            Self::DarkGray => 0x55_55_55,
            Self::Blue => 0x55_55_FF,
            Self::Green => 0x55_FF_55,
            Self::Aqua => 0x55_FF_FF,
            Self::Red => 0xFF_55_55,
            Self::LightPurple => 0xFF_55_FF,
            Self::Yellow => 0xFF_FF_55,
            Self::White => 0xFF_FF_FF,
        }
    }

    /// The name used in JSON and NBT (`dark_blue`, `light_purple`, ...).
    pub const fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::DarkBlue => "dark_blue",
            Self::DarkGreen => "dark_green",
            Self::DarkAqua => "dark_aqua",
            Self::DarkRed => "dark_red",
            Self::DarkPurple => "dark_purple",
            Self::Gold => "gold",
            Self::Gray => "gray",
            Self::DarkGray => "dark_gray",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Aqua => "aqua",
            Self::Red => "red",
            Self::LightPurple => "light_purple",
            Self::Yellow => "yellow",
            Self::White => "white",
        }
    }

    /// The legacy format code character (`0`-`9`, `a`-`f`).
    pub const fn legacy_code(self) -> char {
        match self {
            Self::Black => '0',
            Self::DarkBlue => '1',
            Self::DarkGreen => '2',
            Self::DarkAqua => '3',
            Self::DarkRed => '4',
            Self::DarkPurple => '5',
            Self::Gold => '6',
            Self::Gray => '7',
            Self::DarkGray => '8',
            Self::Blue => '9',
            Self::Green => 'a',
            Self::Aqua => 'b',
            Self::Red => 'c',
            Self::LightPurple => 'd',
            Self::Yellow => 'e',
            Self::White => 'f',
        }
    }

    /// Looks up a named color by its wire name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|color| color.name() == name)
    }

    /// Looks up a named color by a legacy color code character.
    pub fn from_legacy_code(code: char) -> Option<Self> {
        let code = code.to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|color| color.legacy_code() == code)
    }
}

impl fmt::Display for NamedColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A component color: either a named vanilla color or a 24-bit RGB value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextColor {
    Named(NamedColor),
    Rgb(u32),
}

impl TextColor {
    /// A named color.
    pub const fn named(color: NamedColor) -> Self {
        Self::Named(color)
    }

    /// An RGB color. Only the low 24 bits are kept.
    pub const fn rgb(rgb: u32) -> Self {
        Self::Rgb(rgb & 0x00FF_FFFF)
    }

    /// The 24-bit RGB value.
    pub const fn rgb_value(self) -> u32 {
        match self {
            Self::Named(color) => color.rgb(),
            Self::Rgb(rgb) => rgb,
        }
    }

    /// The named color, if this color has one.
    pub const fn named_color(self) -> Option<NamedColor> {
        match self {
            Self::Named(color) => Some(color),
            Self::Rgb(_) => None,
        }
    }

    /// Parses a color name or a `#RRGGBB` string.
    pub fn parse(value: &str) -> Option<Self> {
        if let Some(hex) = value.strip_prefix('#') {
            if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return u32::from_str_radix(hex, 16).ok().map(Self::rgb);
            }
            None
        } else {
            NamedColor::from_name(value).map(Self::Named)
        }
    }
}

impl fmt::Display for TextColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(color) => f.write_str(color.name()),
            Self::Rgb(rgb) => write!(f, "#{rgb:06X}"),
        }
    }
}

impl From<NamedColor> for TextColor {
    fn from(value: NamedColor) -> Self {
        Self::Named(value)
    }
}

impl From<u32> for TextColor {
    fn from(value: u32) -> Self {
        Self::rgb(value)
    }
}

impl From<i32> for TextColor {
    fn from(value: i32) -> Self {
        Self::rgb(value as u32)
    }
}

impl FromStr for TextColor {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or(())
    }
}

/// Formatting and interactivity attached to a component.
///
/// Every field is optional so children can inherit from their parent: `None`
/// means "not set here" rather than "off". [`Style::inherit_from`] resolves
/// that inheritance.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Style {
    pub color: Option<TextColor>,
    pub shadow_color: Option<i32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underlined: Option<bool>,
    pub strikethrough: Option<bool>,
    pub obfuscated: Option<bool>,
    pub font: Option<String>,
    pub insertion: Option<String>,
    pub click_event: Option<ClickEvent>,
    pub hover_event: Option<HoverEvent>,
}

impl Style {
    /// An empty style.
    pub const fn new() -> Self {
        Self {
            color: None,
            shadow_color: None,
            bold: None,
            italic: None,
            underlined: None,
            strikethrough: None,
            obfuscated: None,
            font: None,
            insertion: None,
            click_event: None,
            hover_event: None,
        }
    }

    /// Whether no style property is set.
    pub fn is_empty(&self) -> bool {
        self.color.is_none()
            && self.shadow_color.is_none()
            && self.bold.is_none()
            && self.italic.is_none()
            && self.underlined.is_none()
            && self.strikethrough.is_none()
            && self.obfuscated.is_none()
            && self.font.is_none()
            && self.insertion.is_none()
            && self.click_event.is_none()
            && self.hover_event.is_none()
    }

    /// Resolves `self` on top of an inherited parent style.
    ///
    /// Set fields win; unset fields are copied from the parent.
    pub fn inherit_from(&self, parent: &Style) -> Style {
        Style {
            color: self.color.or(parent.color),
            shadow_color: self.shadow_color.or(parent.shadow_color),
            bold: self.bold.or(parent.bold),
            italic: self.italic.or(parent.italic),
            underlined: self.underlined.or(parent.underlined),
            strikethrough: self.strikethrough.or(parent.strikethrough),
            obfuscated: self.obfuscated.or(parent.obfuscated),
            font: self.font.clone().or_else(|| parent.font.clone()),
            insertion: self.insertion.clone().or_else(|| parent.insertion.clone()),
            click_event: self
                .click_event
                .clone()
                .or_else(|| parent.click_event.clone()),
            hover_event: self
                .hover_event
                .clone()
                .or_else(|| parent.hover_event.clone()),
        }
    }

    /// Sets the text color.
    #[must_use]
    pub fn color(mut self, color: impl Into<TextColor>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Sets the shadow color as a signed ARGB integer.
    ///
    /// A `u32` such as `0xFF00_0000` is accepted as well and keeps its bit
    /// pattern.
    #[must_use]
    pub fn shadow_color(mut self, argb: impl Into<i64>) -> Self {
        self.shadow_color = Some(argb.into() as i32);
        self
    }

    #[must_use]
    pub fn bold(mut self, value: bool) -> Self {
        self.bold = Some(value);
        self
    }

    #[must_use]
    pub fn italic(mut self, value: bool) -> Self {
        self.italic = Some(value);
        self
    }

    #[must_use]
    pub fn underlined(mut self, value: bool) -> Self {
        self.underlined = Some(value);
        self
    }

    #[must_use]
    pub fn strikethrough(mut self, value: bool) -> Self {
        self.strikethrough = Some(value);
        self
    }

    #[must_use]
    pub fn obfuscated(mut self, value: bool) -> Self {
        self.obfuscated = Some(value);
        self
    }

    /// Sets the font resource location (for example `minecraft:default`).
    #[must_use]
    pub fn font(mut self, font: impl Into<String>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Sets the text inserted into chat when shift-clicked.
    #[must_use]
    pub fn insertion(mut self, insertion: impl Into<String>) -> Self {
        self.insertion = Some(insertion.into());
        self
    }

    #[must_use]
    pub fn click_event(mut self, event: ClickEvent) -> Self {
        self.click_event = Some(event);
        self
    }

    #[must_use]
    pub fn hover_event(mut self, event: HoverEvent) -> Self {
        self.hover_event = Some(event);
        self
    }
}

/// What happens when a player clicks a component.
#[derive(Debug, Clone, PartialEq)]
pub enum ClickEvent {
    OpenUrl {
        url: String,
    },
    RunCommand {
        command: String,
    },
    SuggestCommand {
        command: String,
    },
    ChangePage {
        page: i32,
    },
    CopyToClipboard {
        value: String,
    },
    /// A dialog identifier. Inline dialog definitions are not modelled here.
    ShowDialog {
        dialog: String,
    },
    Custom {
        id: String,
        payload: Option<Tag>,
    },
}

impl ClickEvent {
    pub fn open_url(url: impl Into<String>) -> Self {
        Self::OpenUrl { url: url.into() }
    }

    pub fn run_command(command: impl Into<String>) -> Self {
        Self::RunCommand {
            command: command.into(),
        }
    }

    pub fn suggest_command(command: impl Into<String>) -> Self {
        Self::SuggestCommand {
            command: command.into(),
        }
    }

    pub fn change_page(page: i32) -> Self {
        Self::ChangePage { page }
    }

    pub fn copy_to_clipboard(value: impl Into<String>) -> Self {
        Self::CopyToClipboard {
            value: value.into(),
        }
    }

    pub fn show_dialog(dialog: impl Into<String>) -> Self {
        Self::ShowDialog {
            dialog: dialog.into(),
        }
    }

    pub fn custom(id: impl Into<String>) -> Self {
        Self::Custom {
            id: id.into(),
            payload: None,
        }
    }

    /// Attaches a payload to a custom click event; no-op for other actions.
    #[must_use]
    pub fn with_payload(mut self, payload: Tag) -> Self {
        if let Self::Custom { payload: slot, .. } = &mut self {
            *slot = Some(payload);
        }
        self
    }

    /// The vanilla action name.
    pub const fn action(&self) -> &'static str {
        match self {
            Self::OpenUrl { .. } => "open_url",
            Self::RunCommand { .. } => "run_command",
            Self::SuggestCommand { .. } => "suggest_command",
            Self::ChangePage { .. } => "change_page",
            Self::CopyToClipboard { .. } => "copy_to_clipboard",
            Self::ShowDialog { .. } => "show_dialog",
            Self::Custom { .. } => "custom",
        }
    }
}

/// The tooltip shown when a player hovers over a component.
#[derive(Debug, Clone, PartialEq)]
pub enum HoverEvent {
    ShowText {
        value: Box<Component>,
    },
    ShowItem {
        id: String,
        count: Option<i32>,
        /// Data component patch, stored as raw NBT.
        components: Option<Tag>,
    },
    ShowEntity {
        id: String,
        uuid: String,
        name: Option<Box<Component>>,
    },
}

impl HoverEvent {
    pub fn show_text(value: impl Into<Component>) -> Self {
        Self::ShowText {
            value: Box::new(value.into()),
        }
    }

    pub fn show_item(id: impl Into<String>) -> Self {
        Self::ShowItem {
            id: id.into(),
            count: None,
            components: None,
        }
    }

    pub fn show_entity(id: impl Into<String>, uuid: impl Into<String>) -> Self {
        Self::ShowEntity {
            id: id.into(),
            uuid: uuid.into(),
            name: None,
        }
    }

    /// Sets the item count on a `show_item` tooltip; no-op for other actions.
    #[must_use]
    pub fn count(mut self, count: i32) -> Self {
        if let Self::ShowItem { count: slot, .. } = &mut self {
            *slot = Some(count);
        }
        self
    }

    /// Sets the data component patch on a `show_item` tooltip.
    #[must_use]
    pub fn components(mut self, components: Tag) -> Self {
        if let Self::ShowItem {
            components: slot, ..
        } = &mut self
        {
            *slot = Some(components);
        }
        self
    }

    /// Sets the name on a `show_entity` tooltip.
    #[must_use]
    pub fn name(mut self, name: impl Into<Component>) -> Self {
        if let Self::ShowEntity { name: slot, .. } = &mut self {
            *slot = Some(Box::new(name.into()));
        }
        self
    }

    /// The vanilla action name.
    pub const fn action(&self) -> &'static str {
        match self {
            Self::ShowText { .. } => "show_text",
            Self::ShowItem { .. } => "show_item",
            Self::ShowEntity { .. } => "show_entity",
        }
    }
}
