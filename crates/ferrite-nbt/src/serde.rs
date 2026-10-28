//! Serde conversion between Rust values and the NBT model.
//!
//! # Serialization contract
//!
//! | Rust value | NBT tag |
//! |---|---|
//! | `bool` | `Byte(0)` / `Byte(1)` |
//! | `i8`, `u8` | `Byte` |
//! | `i16`, `u16` | `Short` |
//! | `i32`, `u32` | `Int` |
//! | `i64`, `u64` | `Long` |
//! | `f32` | `Float` |
//! | `f64` | `Double` |
//! | `char`, `String`, `&str` | `String` |
//! | `Option<T>` | `Some` serializes as `T`; `None` is omitted from a compound and is an error at the top level or inside a sequence |
//! | newtype struct | the wrapped value |
//! | unit / unit struct | error (NBT has no null or unit) |
//! | struct | `Compound` keyed by field name |
//! | map with `String` keys | `Compound` |
//! | enum unit variant | `String("Variant")` |
//! | enum newtype/tuple/struct variant | `Compound { "Variant": payload }` |
//! | sequence with every element a `Byte` | `ByteArray` |
//! | sequence with every element an `Int` | `IntArray` |
//! | sequence with every element a `Long` | `LongArray` |
//! | empty sequence | `List::End` |
//! | other homogeneous sequence | typed `List` (`Short`, `Float`, `String`, ...) |
//! | heterogeneous sequence | `List::Compound`, wrapping every non-compound element under the empty key (vanilla's representation) |
//! | `Vec<u8>`, `Vec<i8>`, `Vec<bool>` | `ByteArray` |
//! | `Vec<u32>` | `IntArray` |
//! | `Vec<u64>` | `LongArray` |
//! | `Vec<f32>` / `Vec<f64>` | `List::Float` / `List::Double` |
//! | tuple, tuple struct, fixed array | follows the sequence rules |
//! | bytes (`serde_bytes`) | `ByteArray` |
//!
//! Unsigned integers are stored in the corresponding signed tag with a
//! two's-complement cast, so `u64` values above [`i64::MAX`] wrap; reading them
//! back casts in the other direction. `serde_value` uses a sorted map, so
//! compound key order is lexicographic rather than struct field order, and
//! duplicate keys are not possible.
//!
//! # Deserialization contract
//!
//! The reverse direction accepts the tags from the table above. In addition:
//!
//! * `Compound` deserializes into structs and maps with `String` keys; map
//!   keys are always strings in NBT.
//! * `ByteArray`, `IntArray`, `LongArray` and every `List` variant deserialize
//!   as sequences, so `Vec<u8>`, `Vec<i32>`, `Vec<i64>`, fixed arrays and
//!   tuples all work.
//! * `Byte` deserializes as `bool` (`0` is false, anything else true) as well
//!   as `i8`/`u8`. `u8`/`u16`/`u32`/`u64` require the same-width tag
//!   (`Byte`/`Short`/`Int`/`Long`) and reinterpret the bits; the signed and
//!   float types go through serde's usual numeric coercions, so a `Long` tag
//!   can be read as `i64` and an `Int` tag as `i32`.
//! * `Option<T>` always deserializes as `Some`; NBT has no null. Serde supplies
//!   `None` for a missing compound field.
//! * Enum deserialization expects `String` for unit variants and a
//!   single-entry `Compound` for payload variants.
//! * Canonical heterogeneous lists (`List::Compound` wrappers with more than
//!   one recovered type) are unwrapped before being presented as a sequence,
//!   which is what makes tuple variants and mixed sequences round-trip.
//!
//! Wrong tag shapes produce [`Error`] values, for example reading a `String`
//! tag as a struct or calling [`to_tag`] on a non-string-keyed map.
use crate::{Compound, List, Tag};
use ::serde::{
    Serialize,
    de::{self, DeserializeOwned, IntoDeserializer, Visitor},
};
use serde_value::Value;
use std::fmt;

/// Error produced by [`to_tag`] and [`from_tag`].
#[derive(Debug)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}
/// Serializes a Rust value as an NBT [`Tag`] according to the module contract.
pub fn to_tag<T: Serialize>(value: &T) -> Result<Tag, Error> {
    to_nbt(serde_value::to_value(value).map_err(|e| Error(e.to_string()))?)
}

/// Deserializes an NBT [`Tag`] into a Rust value according to the module contract.
pub fn from_tag<T: DeserializeOwned>(tag: Tag) -> Result<T, Error> {
    T::deserialize(Deserializer(tag))
}
fn to_nbt(v: Value) -> Result<Tag, Error> {
    Ok(match v {
        Value::Bool(v) => Tag::Byte(i8::from(v)),
        Value::I8(v) => Tag::Byte(v),
        Value::I16(v) => Tag::Short(v),
        Value::I32(v) => Tag::Int(v),
        Value::I64(v) => Tag::Long(v),
        Value::U8(v) => Tag::Byte(v as i8),
        Value::U16(v) => Tag::Short(v as i16),
        Value::U32(v) => Tag::Int(v as i32),
        Value::U64(v) => Tag::Long(v as i64),
        Value::F32(v) => Tag::Float(v),
        Value::F64(v) => Tag::Double(v),
        Value::Char(v) => Tag::String(v.to_string()),
        Value::String(v) => Tag::String(v),
        Value::Bytes(v) => Tag::ByteArray(v.into_iter().map(|x| x as i8).collect()),
        Value::Option(Some(v)) | Value::Newtype(v) => return to_nbt(*v),
        Value::Option(None) | Value::Unit => return Err(Error("NBT cannot represent null".into())),
        Value::Map(m) => {
            let mut out = Compound::new();
            for (k, v) in m {
                let Value::String(k) = k else {
                    return Err(Error("NBT compound keys must be strings".into()));
                };
                if matches!(v, Value::Option(None)) {
                    continue;
                }
                out.insert(k, to_nbt(v)?);
            }
            Tag::Compound(out)
        }
        Value::Seq(v) => {
            let tags: Vec<Tag> = v.into_iter().map(to_nbt).collect::<Result<_, _>>()?;
            sequence_to_tag(tags)
        }
    })
}

/// Maps a serialized sequence to an array tag when all elements share an
/// integer type, to a typed list when they share any other type, and to a
/// wrapper compound list when the element types are mixed.
fn sequence_to_tag(tags: Vec<Tag>) -> Tag {
    if tags.is_empty() {
        return Tag::List(List::End);
    }
    let id = tags[0].id();
    if tags.iter().all(|tag| tag.id() == id) {
        return match id {
            1 => Tag::ByteArray(
                tags.into_iter()
                    .map(|tag| match tag {
                        Tag::Byte(value) => value,
                        _ => unreachable!("all sequence elements share one type"),
                    })
                    .collect(),
            ),
            3 => Tag::IntArray(
                tags.into_iter()
                    .map(|tag| match tag {
                        Tag::Int(value) => value,
                        _ => unreachable!("all sequence elements share one type"),
                    })
                    .collect(),
            ),
            4 => Tag::LongArray(
                tags.into_iter()
                    .map(|tag| match tag {
                        Tag::Long(value) => value,
                        _ => unreachable!("all sequence elements share one type"),
                    })
                    .collect(),
            ),
            _ => Tag::List(make_list(tags)),
        };
    }

    // Vanilla stores heterogeneous lists as compound lists, leaving compound
    // elements bare and wrapping everything else under the empty key.
    Tag::List(List::Compound(
        tags.into_iter()
            .map(|tag| match tag {
                Tag::Compound(compound) => compound,
                other => {
                    let mut compound = Compound::new();
                    compound.insert(String::new(), other);
                    compound
                }
            })
            .collect(),
    ))
}

fn make_list(tags: Vec<Tag>) -> List {
    macro_rules! list {
        ($variant:ident) => {{
            let mut out = Vec::with_capacity(tags.len());
            for tag in tags {
                let Tag::$variant(value) = tag else {
                    unreachable!("all list elements share one type")
                };
                out.push(value);
            }
            List::$variant(out)
        }};
    }
    match tags[0].id() {
        2 => list!(Short),
        5 => list!(Float),
        6 => list!(Double),
        7 => list!(ByteArray),
        8 => list!(String),
        9 => list!(List),
        10 => list!(Compound),
        11 => list!(IntArray),
        12 => list!(LongArray),
        _ => unreachable!("byte, int and long sequences become arrays"),
    }
}
/// An owned deserializer which interprets byte tags as booleans when requested.
///
/// See the module documentation for the full tag-to-Rust contract.
pub struct Deserializer(pub Tag);
impl<'de> de::Deserializer<'de> for Deserializer {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Tag::Byte(v) => visitor.visit_i8(v),
            Tag::Short(v) => visitor.visit_i16(v),
            Tag::Int(v) => visitor.visit_i32(v),
            Tag::Long(v) => visitor.visit_i64(v),
            Tag::Float(v) => visitor.visit_f32(v),
            Tag::Double(v) => visitor.visit_f64(v),
            Tag::String(v) => visitor.visit_string(v),
            Tag::Compound(v) => {
                let iter = v
                    .into_iter()
                    .map(|(k, v)| (k.into_deserializer(), Deserializer(v)));
                visitor.visit_map(de::value::MapDeserializer::new(iter))
            }
            other => {
                let values = match other {
                    Tag::ByteArray(v) => v.into_iter().map(Tag::Byte).collect(),
                    Tag::IntArray(v) => v.into_iter().map(Tag::Int).collect(),
                    Tag::LongArray(v) => v.into_iter().map(Tag::Long).collect(),
                    Tag::List(v) => list_tags(v),
                    _ => unreachable!(),
                };
                visitor.visit_seq(de::value::SeqDeserializer::new(
                    values.into_iter().map(Deserializer),
                ))
            }
        }
    }
    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Tag::Byte(v) = self.0 {
            visitor.visit_bool(v != 0)
        } else {
            Err(Error("expected byte boolean".into()))
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_u8<V: Visitor<'de>>(self, v: V) -> Result<V::Value, Error> {
        match self.0 {
            Tag::Byte(n) => v.visit_u8(n as u8),
            _ => Err(Error("expected byte".into())),
        }
    }
    fn deserialize_u16<V: Visitor<'de>>(self, v: V) -> Result<V::Value, Error> {
        match self.0 {
            Tag::Short(n) => v.visit_u16(n as u16),
            _ => Err(Error("expected short".into())),
        }
    }
    fn deserialize_u32<V: Visitor<'de>>(self, v: V) -> Result<V::Value, Error> {
        match self.0 {
            Tag::Int(n) => v.visit_u32(n as u32),
            _ => Err(Error("expected int".into())),
        }
    }
    fn deserialize_u64<V: Visitor<'de>>(self, v: V) -> Result<V::Value, Error> {
        match self.0 {
            Tag::Long(n) => v.visit_u64(n as u64),
            _ => Err(Error("expected long".into())),
        }
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Tag::String(name) => visitor.visit_enum(name.into_deserializer()),
            Tag::Compound(map) if map.len() == 1 => {
                let (name, value) = map.into_iter().next().unwrap();
                visitor.visit_enum(Enum { name, value })
            }
            _ => Err(Error("expected enum".into())),
        }
    }
    ::serde::forward_to_deserialize_any! { i8 i16 i32 i64 f32 f64 char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier ignored_any }
}
fn list_tags(v: List) -> Vec<Tag> {
    macro_rules! tags {
        ($v:expr, $kind:ident) => {
            $v.into_iter().map(Tag::$kind).collect()
        };
    }
    match v {
        List::End => Vec::new(),
        List::Byte(v) => tags!(v, Byte),
        List::Short(v) => tags!(v, Short),
        List::Int(v) => tags!(v, Int),
        List::Long(v) => tags!(v, Long),
        List::Float(v) => tags!(v, Float),
        List::Double(v) => tags!(v, Double),
        List::ByteArray(v) => tags!(v, ByteArray),
        List::String(v) => tags!(v, String),
        List::List(v) => tags!(v, List),
        List::Compound(values) => unwrap_heterogeneous(values),
        List::IntArray(v) => tags!(v, IntArray),
        List::LongArray(v) => tags!(v, LongArray),
    }
}

/// A canonical heterogeneous list is a compound list whose elements are plain
/// compounds plus single-entry compounds with an empty key. When unwrapping the
/// wrappers yields more than one tag type, the list really is heterogeneous and
/// the wrappers are removed for deserialization.
fn unwrap_heterogeneous(values: Vec<Compound>) -> Vec<Tag> {
    let mut ids = Vec::with_capacity(values.len());
    let mut unwrapped = Vec::with_capacity(values.len());
    for compound in &values {
        match wrapper_value(compound) {
            Some(inner) if !matches!(inner, Tag::Compound(_)) => {
                ids.push(inner.id());
                unwrapped.push(inner.clone());
            }
            _ => {
                // 10 is the NBT compound tag id.
                ids.push(10);
                unwrapped.push(Tag::Compound(compound.clone()));
            }
        }
    }
    if ids.windows(2).any(|pair| pair[0] != pair[1]) {
        unwrapped
    } else {
        values.into_iter().map(Tag::Compound).collect()
    }
}

fn wrapper_value(compound: &Compound) -> Option<&Tag> {
    if compound.len() != 1 {
        return None;
    }
    let (key, value) = compound.iter().next()?;
    key.is_empty().then_some(value)
}
impl<'de> IntoDeserializer<'de, Error> for Deserializer {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}

struct Enum {
    name: String,
    value: Tag,
}
impl<'de> de::EnumAccess<'de> for Enum {
    type Error = Error;
    type Variant = Deserializer;
    fn variant_seed<V: de::DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        Ok((
            seed.deserialize(self.name.into_deserializer())?,
            Deserializer(self.value),
        ))
    }
}
impl<'de> de::VariantAccess<'de> for Deserializer {
    type Error = Error;
    fn unit_variant(self) -> Result<(), Error> {
        Err(Error("expected unit enum".into()))
    }
    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(self)
    }
    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_any(self, visitor)
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_any(self, visitor)
    }
}
