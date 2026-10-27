use ferrite_nbt::{Compound, List, Tag, binary};
#[test]
fn binary_roundtrip_named_and_network() {
    let mut map = Compound::new();
    map.insert("a\0??".into(), Tag::String("\0??".into()));
    map.insert("list".into(), Tag::List(List::Int(vec![1, -2])));
    let tag = Tag::Compound(map);
    let bytes = binary::write_named("root", &tag).unwrap();
    assert_eq!(
        binary::read_named(&bytes).unwrap(),
        ("root".into(), tag.clone())
    );
    let bytes = binary::write_network(&tag).unwrap();
    assert_eq!(binary::read_network(&bytes).unwrap(), tag);
}
#[test]
fn malformed_input_does_not_panic() {
    let mut seed = 1u32;
    for len in 0..256 {
        let mut bytes = vec![0; len];
        for b in &mut bytes {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *b = (seed >> 24) as u8;
        }
        let _ = binary::read_named(&bytes);
    }
    assert!(binary::read_named(&[10, 0, 0, 7, 0, 1, b'a', 0x7f, 0xff, 0xff, 0xff]).is_err());
}

#[test]
fn compression_and_all_types_roundtrip() {
    let mut root = Compound::new();
    for (name, tag) in [
        ("byte", Tag::Byte(-1)),
        ("short", Tag::Short(-300)),
        ("int", Tag::Int(-12345)),
        ("long", Tag::Long(-123456789)),
        ("float", Tag::Float(1.25)),
        ("double", Tag::Double(-2.5)),
        ("bytes", Tag::ByteArray(vec![-1, 0, 1])),
        ("ints", Tag::IntArray(vec![-2, 4])),
        ("longs", Tag::LongArray(vec![-2, 4])),
        ("empty", Tag::List(List::String(vec![]))),
        ("nested", Tag::List(List::Compound(vec![Compound::new()]))),
    ] {
        root.insert(name.into(), tag);
    }
    let tag = Tag::Compound(root);
    for compression in [
        binary::Compression::None,
        binary::Compression::Gzip,
        binary::Compression::Zlib,
    ] {
        let data = binary::write_compressed("", &tag, compression).unwrap();
        assert_eq!(
            binary::read_compressed(&data, compression).unwrap(),
            (String::new(), tag.clone())
        );
    }
}
#[test]
fn reject_invalid_utf8_and_lengths() {
    for input in [
        &[8, 0, 0, 0, 1, 0][..],
        &[9, 0, 0, 0, 0, 0, 1][..],
        &[11, 0, 0, 0, 0, 0, 2, 0][..],
    ] {
        assert!(binary::read_named(input).is_err());
    }
}

#[test]
fn snbt_roundtrip() {
    let source = r#"{name:"hello \"world\"",byte:1b,short:-2s,int:3,long:4L,float:1.5f,double:2.5d,bytes:[B;1b,-2b],ints:[I;1,2],longs:[L;1L,2L],list:[{x:1},{x:2}]}"#;
    let tag = ferrite_nbt::snbt::parse(source).unwrap();
    let formatted = ferrite_nbt::snbt::format(&tag);
    assert_eq!(ferrite_nbt::snbt::parse(&formatted).unwrap(), tag);
}

#[test]
fn rejects_large_list_even_when_input_is_long_enough() {
    let mut data = vec![9, 0, 0, 1];
    data.extend_from_slice(&1_000_001i32.to_be_bytes());
    data.resize(data.len() + 1_000_001, 0);
    assert!(binary::read_named(&data).is_err());
}
