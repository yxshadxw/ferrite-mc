//! Minecraft Java 26.3 text components.
//!
//! A [`Component`] is a piece of chat text: one of six content kinds (`text`,
//! `translatable`, `keybind`, `score`, `selector`, `nbt`), an optional
//! [`Style`], and a list of child components (the `extra` field on the wire).
//!
//! Three representations are supported:
//!
//! * JSON via [`Component::from_json`]/[`Component::to_json`]. The modern
//!   object shape is used; the vanilla shorthands (a bare string, or an array
//!   whose first element becomes the root and the rest its children) are
//!   accepted.
//! * NBT via [`Component::to_nbt`]/[`Component::from_nbt`], plus
//!   [`Component::encode_network`] for the nameless-root network form. A plain
//!   text component with no style or children collapses to a bare string tag,
//!   as vanilla does.
//! * Legacy `§`/`&` formatting codes via [`Component::from_legacy`],
//!   [`Component::to_legacy`] and [`Component::to_plain_text`].
//!
//! ```
//! use ferrite_text::{Component, NamedColor};
//!
//! let component = Component::text("hi").color(NamedColor::Red).bold(true);
//! assert_eq!(component.to_plain_text(), "hi");
//! assert_eq!(component.to_legacy(), "§c§lhi");
//! ```

mod component;
mod json;
mod legacy;
mod nbt;
mod style;
mod value;

pub use component::{Component, Content, NbtSource, TextRun};
pub use style::{ClickEvent, HoverEvent, NamedColor, Style, TextColor};

/// Errors produced while parsing or encoding a text component.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The component data did not match the expected shape.
    #[error("invalid text component: {0}")]
    Invalid(String),
    /// A JSON syntax or type error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Binary NBT encoding or decoding failed.
    #[error(transparent)]
    Nbt(#[from] ferrite_nbt::binary::Error),
}

impl Error {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

/// Result type used by text component parsing and encoding.
pub type Result<T> = std::result::Result<T, Error>;
