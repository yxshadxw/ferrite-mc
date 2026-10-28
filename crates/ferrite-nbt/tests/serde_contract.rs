#![cfg(feature = "serde")]

use ferrite_nbt::serde::{from_tag, to_tag};
use ferrite_nbt::{Compound, List, Tag};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;
use std::fmt::Debug;

fn roundtrip<T>(value: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + Debug,
{
    let tag = to_tag(value).unwrap_or_else(|error| panic!("{value:?}: {error}"));
    let actual = from_tag::<T>(tag.clone()).unwrap_or_else(|error| panic!("{tag:?}: {error}"));
    assert_eq!(&actual, value);
}

fn as_compound(tag: &Tag) -> &Compound {
    match tag {
        Tag::Compound(compound) => compound,
        other => panic!("expected a compound, got {other:?}"),
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Scalars {
    flag: bool,
    byte: i8,
    unsigned_byte: u8,
    short: i16,
    unsigned_short: u16,
    int: i32,
    unsigned_int: u32,
    long: i64,
    unsigned_long: u64,
    float: f32,
    double: f64,
    character: char,
    text: String,
    bytes: Vec<u8>,
    ints: Vec<i32>,
    longs: Vec<i64>,
    strings: Vec<String>,
}

#[test]
fn scalar_and_sequence_mapping_follows_the_contract() {
    let value = Scalars {
        flag: true,
        byte: -128,
        unsigned_byte: 255,
        short: -32768,
        unsigned_short: 65535,
        int: i32::MIN,
        unsigned_int: u32::MAX,
        long: i64::MIN,
        unsigned_long: u64::MAX,
        float: 1.25,
        double: -2.5,
        character: '☃',
        text: "hello".into(),
        bytes: vec![0, 127, 128, 255],
        ints: vec![1, -2, i32::MAX],
        longs: vec![1, -2, i64::MIN],
        strings: vec!["a".into(), "b c".into()],
    };
    let tag = to_tag(&value).unwrap();
    let map = as_compound(&tag);

    assert_eq!(map.get("flag"), Some(&Tag::Byte(1)));
    assert_eq!(map.get("byte"), Some(&Tag::Byte(-128)));
    assert_eq!(map.get("unsigned_byte"), Some(&Tag::Byte(-1)));
    assert_eq!(map.get("short"), Some(&Tag::Short(-32768)));
    assert_eq!(map.get("unsigned_short"), Some(&Tag::Short(-1)));
    assert_eq!(map.get("int"), Some(&Tag::Int(i32::MIN)));
    assert_eq!(map.get("unsigned_int"), Some(&Tag::Int(-1)));
    assert_eq!(map.get("long"), Some(&Tag::Long(i64::MIN)));
    assert_eq!(map.get("unsigned_long"), Some(&Tag::Long(-1)));
    assert_eq!(map.get("float"), Some(&Tag::Float(1.25)));
    assert_eq!(map.get("double"), Some(&Tag::Double(-2.5)));
    assert_eq!(map.get("character"), Some(&Tag::String("☃".into())));
    assert_eq!(map.get("text"), Some(&Tag::String("hello".into())));
    assert_eq!(
        map.get("bytes"),
        Some(&Tag::ByteArray(vec![0, 127, -128, -1]))
    );
    assert_eq!(map.get("ints"), Some(&Tag::IntArray(vec![1, -2, i32::MAX])));
    assert_eq!(
        map.get("longs"),
        Some(&Tag::LongArray(vec![1, -2, i64::MIN]))
    );
    assert_eq!(
        map.get("strings"),
        Some(&Tag::List(List::String(vec!["a".into(), "b c".into()])))
    );

    roundtrip(&value);
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Inner {
    id: i32,
    label: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Outer {
    inner: Inner,
    optional: Option<i32>,
    nested_optional: Option<Inner>,
    items: Vec<Inner>,
}

#[test]
fn nested_structs_and_options_roundtrip() {
    for (optional, nested_optional) in [
        (None, None),
        (
            Some(7),
            Some(Inner {
                id: 8,
                label: "nested".into(),
            }),
        ),
    ] {
        let has_optional = optional.is_some();
        let has_nested_optional = nested_optional.is_some();
        let value = Outer {
            inner: Inner {
                id: 1,
                label: "root".into(),
            },
            optional,
            nested_optional,
            items: vec![
                Inner {
                    id: 2,
                    label: "two".into(),
                },
                Inner {
                    id: 3,
                    label: "three".into(),
                },
            ],
        };
        let tag = to_tag(&value).unwrap();
        let map = as_compound(&tag);
        assert_eq!(
            map.get("inner"),
            Some(&Tag::Compound(
                [
                    ("id".to_owned(), Tag::Int(1)),
                    ("label".to_owned(), Tag::String("root".into())),
                ]
                .into_iter()
                .collect()
            ))
        );
        assert_eq!(
            map.get("items"),
            Some(&Tag::List(List::Compound(vec![
                [
                    ("id".to_owned(), Tag::Int(2)),
                    ("label".to_owned(), Tag::String("two".into())),
                ]
                .into_iter()
                .collect(),
                [
                    ("id".to_owned(), Tag::Int(3)),
                    ("label".to_owned(), Tag::String("three".into())),
                ]
                .into_iter()
                .collect(),
            ])))
        );
        assert_eq!(map.contains_key("optional"), has_optional);
        assert_eq!(map.contains_key("nested_optional"), has_nested_optional);
        roundtrip(&value);
    }
}

#[test]
fn options_are_transparent_but_none_is_not_representable_outside_compounds() {
    assert_eq!(to_tag(&Some(5i32)).unwrap(), Tag::Int(5));
    assert_eq!(from_tag::<Option<i32>>(Tag::Int(5)).unwrap(), Some(5));
    assert!(to_tag(&Option::<i32>::None).is_err());
    assert!(to_tag(&vec![Some(1i32), None]).is_err());
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Mode {
    Idle,
    Number(i32),
    Pair(i32, String),
    Named { id: i32, name: String },
}

#[test]
fn enum_variants_map_to_strings_and_single_entry_compounds() {
    assert_eq!(to_tag(&Mode::Idle).unwrap(), Tag::String("Idle".into()));

    let tag = to_tag(&Mode::Number(42)).unwrap();
    assert_eq!(as_compound(&tag).get("Number"), Some(&Tag::Int(42)));

    let tag = to_tag(&Mode::Pair(1, "x".into())).unwrap();
    let payload = as_compound(&tag).get("Pair").unwrap();
    let Tag::List(List::Compound(items)) = payload else {
        panic!("expected a heterogeneous wrapper list, got {payload:?}");
    };
    assert_eq!(items[0].get(""), Some(&Tag::Int(1)));
    assert_eq!(items[1].get(""), Some(&Tag::String("x".into())));

    let tag = to_tag(&Mode::Named {
        id: 3,
        name: "three".into(),
    })
    .unwrap();
    let Tag::Compound(named) = as_compound(&tag).get("Named").unwrap() else {
        panic!("expected a compound payload");
    };
    assert_eq!(named.get("id"), Some(&Tag::Int(3)));
    assert_eq!(named.get("name"), Some(&Tag::String("three".into())));

    for mode in [
        Mode::Idle,
        Mode::Number(42),
        Mode::Pair(-1, "left".into()),
        Mode::Named {
            id: 9,
            name: "nine".into(),
        },
    ] {
        roundtrip(&mode);
    }
}

#[test]
fn maps_with_string_keys_become_compounds() {
    let map = BTreeMap::from([("alpha".to_owned(), 1i32), ("beta".to_owned(), -2)]);
    assert_eq!(
        to_tag(&map).unwrap(),
        Tag::Compound(
            [
                ("alpha".to_owned(), Tag::Int(1)),
                ("beta".to_owned(), Tag::Int(-2)),
            ]
            .into_iter()
            .collect()
        )
    );
    roundtrip(&map);

    let nested = BTreeMap::from([
        (
            "one".to_owned(),
            Inner {
                id: 1,
                label: "first".into(),
            },
        ),
        (
            "two".to_owned(),
            Inner {
                id: 2,
                label: "second".into(),
            },
        ),
    ]);
    roundtrip(&nested);

    let bad = BTreeMap::from([(1i32, 2i32)]);
    assert!(to_tag(&bad).is_err());
}

#[test]
fn sequences_use_arrays_for_byte_int_and_long_elements() {
    assert_eq!(
        to_tag(&vec![0u8, 127, 255]).unwrap(),
        Tag::ByteArray(vec![0, 127, -1])
    );
    assert_eq!(
        to_tag(&vec![-128i8, 0, 127]).unwrap(),
        Tag::ByteArray(vec![-128, 0, 127])
    );
    assert_eq!(
        to_tag(&vec![true, false, true]).unwrap(),
        Tag::ByteArray(vec![1, 0, 1])
    );
    assert_eq!(
        to_tag(&vec![1u16, 2]).unwrap(),
        Tag::List(List::Short(vec![1, 2]))
    );
    assert_eq!(to_tag(&vec![1u32, 2]).unwrap(), Tag::IntArray(vec![1, 2]));
    assert_eq!(to_tag(&vec![1u64, 2]).unwrap(), Tag::LongArray(vec![1, 2]));
    assert_eq!(
        to_tag(&vec![1.5f32, -0.5]).unwrap(),
        Tag::List(List::Float(vec![1.5, -0.5]))
    );
    assert_eq!(
        to_tag(&vec![1.5f64, -0.5]).unwrap(),
        Tag::List(List::Double(vec![1.5, -0.5]))
    );
    assert_eq!(
        to_tag(&vec![vec![1u8, 2], vec![3]]).unwrap(),
        Tag::List(List::ByteArray(vec![vec![1, 2], vec![3]]))
    );
    assert_eq!(to_tag(&Vec::<i32>::new()).unwrap(), Tag::List(List::End));
    assert_eq!(to_tag(&Vec::<u8>::new()).unwrap(), Tag::List(List::End));

    roundtrip(&vec![0u8, 127, 255]);
    roundtrip(&vec![-128i8, 0, 127]);
    roundtrip(&vec![true, false, true]);
    roundtrip(&vec![1u16, 2]);
    roundtrip(&vec![1u32, 2]);
    roundtrip(&vec![1u64, u64::MAX]);
    roundtrip(&vec![1.5f32, -0.5]);
    roundtrip(&vec![1.5f64, -0.5]);
    roundtrip(&vec![vec![1u8, 2], vec![3]]);
    roundtrip(&Vec::<i32>::new());
    roundtrip(&Vec::<u8>::new());
    roundtrip(&[1u8, 2, 3, 4]);
    roundtrip(&(1u8, 2u8));
}

#[test]
fn heterogeneous_sequences_roundtrip_through_wrapper_lists() {
    let value = vec![
        Mixed::Number(1),
        Mixed::Text("two".into()),
        Mixed::Number(3),
    ];
    let tag = to_tag(&value).unwrap();
    let Tag::List(List::Compound(items)) = &tag else {
        panic!("expected a wrapper compound list, got {tag:?}");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].get(""), Some(&Tag::Int(1)));
    assert_eq!(items[1].get(""), Some(&Tag::String("two".into())));
    assert_eq!(items[2].get(""), Some(&Tag::Int(3)));
    roundtrip(&value);

    roundtrip(&(1i32, "one".to_owned()));
    roundtrip(&(1i32, "one".to_owned(), 2.5f64));
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum Mixed {
    Number(i32),
    Text(String),
}

#[test]
fn wrong_tag_shapes_are_errors() {
    assert!(from_tag::<String>(Tag::Int(1)).is_err());
    assert!(from_tag::<bool>(Tag::Int(1)).is_err());
    assert!(from_tag::<u8>(Tag::Int(1)).is_err());
    assert!(from_tag::<u8>(Tag::Short(1)).is_err());
    assert!(from_tag::<u32>(Tag::Long(1)).is_err());
    assert!(from_tag::<Mode>(Tag::Int(1)).is_err());
    assert!(from_tag::<Mode>(Tag::String("NoSuchVariant".into())).is_err());
    assert!(from_tag::<Inner>(Tag::String("not an object".into())).is_err());
    assert!(from_tag::<Vec<i32>>(Tag::Int(1)).is_err());
    assert!(from_tag::<Vec<u8>>(Tag::IntArray(vec![1, 2])).is_err());
    assert!(to_tag(&()).is_err());
    assert!(to_tag(&BTreeMap::from([(1i32, 2i32)])).is_err());
}

#[test]
fn numeric_coercions_are_available_when_deserializing() {
    assert_eq!(from_tag::<i64>(Tag::Int(5)).unwrap(), 5);
    assert_eq!(from_tag::<i16>(Tag::Byte(5)).unwrap(), 5);
    assert_eq!(from_tag::<f64>(Tag::Float(1.5)).unwrap(), 1.5);
    assert_eq!(from_tag::<f32>(Tag::Double(1.5)).unwrap(), 1.5);
    assert_eq!(from_tag::<u64>(Tag::Long(-1)).unwrap(), u64::MAX);
    assert_eq!(from_tag::<u8>(Tag::Byte(-1)).unwrap(), 255);
}

#[test]
fn compound_key_order_is_lexicographic() {
    #[derive(Serialize)]
    struct Ordered {
        zebra: i32,
        apple: i32,
        mango: i32,
    }
    let tag = to_tag(&Ordered {
        zebra: 1,
        apple: 2,
        mango: 3,
    })
    .unwrap();
    let keys: Vec<&str> = as_compound(&tag).keys().map(String::as_str).collect();
    assert_eq!(keys, ["apple", "mango", "zebra"]);
}
