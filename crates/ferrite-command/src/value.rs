//! Parsed argument values.

/// A parsed block state: `minecraft:stone[axis=y]{...}` broken into its shape.
///
/// Only the syntax is validated, matching the "shape only" treatment of
/// `minecraft:block_state` in this crate: names, property keys and values are kept as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockState {
    /// Block name as written, including any namespace.
    pub name: String,
    /// State properties in the order they were written.
    pub properties: Vec<(String, String)>,
    /// Raw NBT block entity data, without the braces, when present.
    pub nbt: Option<String>,
}

/// The value an argument parser produced.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentValue {
    Bool(bool),
    /// `brigadier:integer`, `brigadier:long`, `minecraft:time` and friends.
    Integer(i64),
    /// `brigadier:float`, `brigadier:double`.
    Float(f64),
    /// Text arguments: strings, messages, resource locations, dimensions, gamemodes,
    /// components, game profiles, opaque parsers.
    String(String),
    /// A vanilla entity selector (`@e[type=pig]`) or a player name.
    Entity(String),
    BlockPos { x: i32, y: i32, z: i32 },
    ColumnPos { x: i32, z: i32 },
    Vec3 { x: f64, y: f64, z: f64 },
    Vec2 { x: f64, z: f64 },
    BlockState(BlockState),
}

impl ArgumentValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(v) => Some(*v),
            Self::Integer(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Text of string-like arguments, including entity selectors.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(v) | Self::Entity(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_block_state(&self) -> Option<&BlockState> {
        match self {
            Self::BlockState(v) => Some(v),
            _ => None,
        }
    }
}

/// One parsed argument plus the input range it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedArgument {
    /// Character offset where the argument text starts.
    pub start: usize,
    /// Character offset one past the argument text.
    pub end: usize,
    pub value: ArgumentValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_only_match_their_variant() {
        assert_eq!(ArgumentValue::Integer(3).as_i64(), Some(3));
        assert_eq!(ArgumentValue::Integer(3).as_f64(), Some(3.0));
        assert_eq!(ArgumentValue::Bool(true).as_i64(), None);
        assert_eq!(ArgumentValue::Entity("@a".into()).as_str(), Some("@a"));
        assert_eq!(ArgumentValue::Float(1.5).as_f64(), Some(1.5));
    }
}
