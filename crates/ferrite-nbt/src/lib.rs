//! Named Binary Tag (NBT) data model.
//!
//! Binary IO (Java Edition big-endian, network "nameless root" form, gzip/zlib) and SNBT
//! live in submodules added on top of this model.

use indexmap::IndexMap;

/// An ordered compound tag. Insertion order is kept so re-encoding is byte-stable.
pub type Compound = IndexMap<String, Tag>;

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(List),
    Compound(Compound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

/// A homogeneous list. Kept typed so empty lists still round-trip their element type.
#[derive(Debug, Clone, PartialEq)]
pub enum List {
    End,
    Byte(Vec<i8>),
    Short(Vec<i16>),
    Int(Vec<i32>),
    Long(Vec<i64>),
    Float(Vec<f32>),
    Double(Vec<f64>),
    ByteArray(Vec<Vec<i8>>),
    String(Vec<String>),
    List(Vec<List>),
    Compound(Vec<Compound>),
    IntArray(Vec<Vec<i32>>),
    LongArray(Vec<Vec<i64>>),
}

impl Tag {
    pub const fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(_) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }
}

impl From<String> for Tag {
    fn from(v: String) -> Self {
        Tag::String(v)
    }
}
impl From<&str> for Tag {
    fn from(v: &str) -> Self {
        Tag::String(v.to_owned())
    }
}
impl From<i8> for Tag {
    fn from(v: i8) -> Self {
        Tag::Byte(v)
    }
}
impl From<bool> for Tag {
    fn from(v: bool) -> Self {
        Tag::Byte(v as i8)
    }
}
impl From<i32> for Tag {
    fn from(v: i32) -> Self {
        Tag::Int(v)
    }
}
impl From<i64> for Tag {
    fn from(v: i64) -> Self {
        Tag::Long(v)
    }
}
impl From<f32> for Tag {
    fn from(v: f32) -> Self {
        Tag::Float(v)
    }
}
impl From<f64> for Tag {
    fn from(v: f64) -> Self {
        Tag::Double(v)
    }
}
impl From<Compound> for Tag {
    fn from(v: Compound) -> Self {
        Tag::Compound(v)
    }
}
pub mod binary;
pub mod snbt;
