//! SNBT grammar acceptance/rejection tables and round-trip properties.
//!
//! The accepted forms mirror the vanilla 26.3 `SnbtGrammar` behavior, except
//! for the deliberate differences documented on [`ferrite_nbt::snbt`]: lone
//! surrogate escapes are rejected, and the six non-finite float spellings are
//! accepted so that binary NBT values survive a format/parse round-trip.

use ferrite_nbt::{Compound, List, Tag, snbt};

fn compound(entries: &[(&str, Tag)]) -> Compound {
    let mut map = Compound::new();
    for (key, value) in entries {
        map.insert((*key).to_owned(), value.clone());
    }
    map
}

fn tag_compound(entries: &[(&str, Tag)]) -> Tag {
    Tag::Compound(compound(entries))
}

fn accept(input: &str, expected: Tag) {
    match snbt::parse(input) {
        Ok(actual) => assert!(same_tag(&actual, &expected), "input {input:?}: {actual:?}"),
        Err(error) => panic!("input {input:?} should parse, got {error}"),
    }
}

fn reject(input: &str) {
    if let Ok(tag) = snbt::parse(input) {
        panic!("input {input:?} should be rejected, parsed as {tag:?}");
    }
}

#[test]
fn accepts_integers_and_suffixes() {
    for (input, expected) in [
        ("0", Tag::Int(0)),
        ("-0", Tag::Int(0)),
        ("+42", Tag::Int(42)),
        ("1_2_3", Tag::Int(123)),
        ("1__2", Tag::Int(12)),
        ("1___2", Tag::Int(12)),
        ("0x1__2", Tag::Int(18)),
        ("0b1__0", Tag::Int(2)),
        ("2147483647", Tag::Int(i32::MAX)),
        ("-2147483648", Tag::Int(i32::MIN)),
        ("0x1A", Tag::Int(26)),
        ("0X1a", Tag::Int(26)),
        ("0x7FFFFFFF", Tag::Int(i32::MAX)),
        ("0x80000000", Tag::Int(i32::MIN)),
        ("0xFFFFFFFF", Tag::Int(-1)),
        ("0b101", Tag::Int(5)),
        ("0B101", Tag::Int(5)),
        ("0b1111_1111", Tag::Int(255)),
        ("0x1_0000", Tag::Int(65536)),
        ("0x1b", Tag::Int(27)),
        ("0x1f", Tag::Int(31)),
        ("0x1d", Tag::Int(29)),
        // Vanilla backtracks `0b` with no binary digits into byte 0.
        ("0b", Tag::Byte(0)),
        ("0B", Tag::Byte(0)),
        ("1b", Tag::Byte(1)),
        ("1B", Tag::Byte(1)),
        ("-1sb", Tag::Byte(-1)),
        ("240ub", Tag::Byte(-16)),
        ("255ub", Tag::Byte(-1)),
        ("-128sb", Tag::Byte(-128)),
        ("127sb", Tag::Byte(127)),
        ("1s", Tag::Short(1)),
        ("1S", Tag::Short(1)),
        ("-2s", Tag::Short(-2)),
        ("65535us", Tag::Short(-1)),
        ("1i", Tag::Int(1)),
        ("1I", Tag::Int(1)),
        ("4294967295ui", Tag::Int(-1)),
        ("1l", Tag::Long(1)),
        ("1L", Tag::Long(1)),
        ("18446744073709551615ul", Tag::Long(-1)),
        ("1us", Tag::Short(1)),
        ("1ui", Tag::Int(1)),
        ("1ul", Tag::Long(1)),
        ("1ss", Tag::Short(1)),
        ("1si", Tag::Int(1)),
        ("1sl", Tag::Long(1)),
        ("1sB", Tag::Byte(1)),
        ("1SB", Tag::Byte(1)),
        ("1uB", Tag::Byte(1)),
        ("0x1sb", Tag::Byte(1)),
        ("0x1ub", Tag::Byte(1)),
        ("0x1us", Tag::Short(1)),
        ("0x1ui", Tag::Int(1)),
        ("0x1ul", Tag::Long(1)),
        ("0x1ss", Tag::Short(1)),
        ("0x1si", Tag::Int(1)),
        ("0x1sl", Tag::Long(1)),
        ("0x80ub", Tag::Byte(-128)),
        ("0x1l", Tag::Long(1)),
        ("0b1s", Tag::Short(1)),
        ("0b1l", Tag::Long(1)),
        ("0b1ub", Tag::Byte(1)),
        ("0b1ul", Tag::Long(1)),
        ("+0x1", Tag::Int(1)),
        ("+0x1sb", Tag::Byte(1)),
        ("-0x1sb", Tag::Byte(-1)),
        ("-0x1si", Tag::Int(-1)),
        ("-0x1sl", Tag::Long(-1)),
        ("-0x1ss", Tag::Short(-1)),
        ("-0b1sb", Tag::Byte(-1)),
        ("[B;0xFF]", Tag::ByteArray(vec![-1])),
    ] {
        accept(input, expected);
    }
}

#[test]
fn accepts_floats_and_non_finite_values() {
    for (input, expected) in [
        ("1.5", Tag::Double(1.5)),
        (".5", Tag::Double(0.5)),
        ("1.", Tag::Double(1.0)),
        ("00.0", Tag::Double(0.0)),
        ("01.5", Tag::Double(1.5)),
        ("1e3", Tag::Double(1000.0)),
        ("1.2e3", Tag::Double(1200.0)),
        ("1.2E+3", Tag::Double(1200.0)),
        ("12000e-1", Tag::Double(1200.0)),
        ("1_2.3_4", Tag::Double(12.34)),
        ("1__2.5", Tag::Double(12.5)),
        ("1e1_0", Tag::Double(1e10)),
        ("1e5__2", Tag::Double(1e52)),
        ("1e-400", Tag::Double(0.0)),
        ("1e308", Tag::Double(1e308)),
        ("1f", Tag::Float(1.0)),
        ("1F", Tag::Float(1.0)),
        ("1d", Tag::Double(1.0)),
        ("1D", Tag::Double(1.0)),
        ("1.5f", Tag::Float(1.5)),
        ("1.5F", Tag::Float(1.5)),
        ("1.5d", Tag::Double(1.5)),
        ("1e5f", Tag::Float(100_000.0)),
        (".1f", Tag::Float(0.1)),
        ("1.f", Tag::Float(1.0)),
        ("1e-50f", Tag::Float(0.0)),
        ("1e-45f", Tag::Float(f32::from_bits(1))),
        ("3.4028235e38f", Tag::Float(f32::MAX)),
        ("NaNf", Tag::Float(f32::NAN)),
        ("NaNd", Tag::Double(f64::NAN)),
        ("Infinityf", Tag::Float(f32::INFINITY)),
        ("Infinityd", Tag::Double(f64::INFINITY)),
        ("-Infinityf", Tag::Float(f32::NEG_INFINITY)),
        ("-Infinityd", Tag::Double(f64::NEG_INFINITY)),
    ] {
        accept(input, expected);
    }
    for input in ["-0.0", "-0.0d", "-0.0f"] {
        let value = match snbt::parse(input).unwrap() {
            Tag::Double(value) => value,
            Tag::Float(value) => f64::from(value),
            other => panic!("{input:?} parsed as {other:?}"),
        };
        assert_eq!(value.to_bits(), (-0.0f64).to_bits(), "input {input:?}");
    }
}

#[test]
fn accepts_booleans_strings_and_operations() {
    for (input, expected) in [
        ("true", Tag::Byte(1)),
        ("TRUE", Tag::Byte(1)),
        ("TrUe", Tag::Byte(1)),
        ("false", Tag::Byte(0)),
        ("False", Tag::Byte(0)),
        ("truex", Tag::String("truex".into())),
        ("_1", Tag::String("_1".into())),
        ("_1_", Tag::String("_1_".into())),
        ("ABC", Tag::String("ABC".into())),
        ("a-b", Tag::String("a-b".into())),
        ("a+b", Tag::String("a+b".into())),
        ("a.b", Tag::String("a.b".into())),
        ("a_b", Tag::String("a_b".into())),
        ("_.", Tag::String("_.".into())),
        ("NaN", Tag::String("NaN".into())),
        ("Infinity", Tag::String("Infinity".into())),
        ("inf", Tag::String("inf".into())),
        ("\"double\"", Tag::String("double".into())),
        ("'single'", Tag::String("single".into())),
        ("\"\"", Tag::String(String::new())),
        ("''", Tag::String(String::new())),
        ("\"a'b\"", Tag::String("a'b".into())),
        ("'a\"b'", Tag::String("a\"b".into())),
        ("\"\\\"\"", Tag::String("\"".into())),
        ("'\\''", Tag::String("'".into())),
        ("\"\\\\\"", Tag::String("\\".into())),
        (
            "\"\\n\\t\\r\\b\\f\\s\"",
            Tag::String("\n\t\r\u{8}\u{c} ".into()),
        ),
        ("\"\\x21\"", Tag::String("!".into())),
        ("\"\\x7f\"", Tag::String("\u{7f}".into())),
        ("\"\\u2603\"", Tag::String("\u{2603}".into())),
        ("\"\\uD83D\\uDE00\"", Tag::String("\u{1f600}".into())),
        ("\"\\U0001F600\"", Tag::String("\u{1f600}".into())),
        ("\"\\N{SNOWMAN}\"", Tag::String("\u{2603}".into())),
        ("\"\\N{Snowman}\"", Tag::String("\u{2603}".into())),
        ("\"\\N{ SNOWMAN }\"", Tag::String("\u{2603}".into())),
        ("\"line\nbreak\"", Tag::String("line\nbreak".into())),
        ("bool(0)", Tag::Byte(0)),
        ("bool(1)", Tag::Byte(1)),
        ("bool(0.5)", Tag::Byte(1)),
        ("bool(true)", Tag::Byte(1)),
        ("bool(false)", Tag::Byte(0)),
        ("bool(bool(1))", Tag::Byte(1)),
        (
            "uuid(\"00112233-4455-6677-8899-aabbccddeeff\")",
            Tag::IntArray(vec![
                0x0011_2233,
                0x4455_6677,
                0x8899_aabbu32 as i32,
                0xccdd_eeffu32 as i32,
            ]),
        ),
        (
            "uuid('00112233-4455-6677-8899-AABBCCDDEEFF')",
            Tag::IntArray(vec![
                0x0011_2233,
                0x4455_6677,
                0x8899_aabbu32 as i32,
                0xccdd_eeffu32 as i32,
            ]),
        ),
    ] {
        accept(input, expected);
    }
}

#[test]
fn accepts_arrays_lists_and_compounds() {
    for (input, expected) in [
        ("[B;]", Tag::ByteArray(Vec::new())),
        ("[B; ]", Tag::ByteArray(Vec::new())),
        ("[I;]", Tag::IntArray(Vec::new())),
        ("[L;]", Tag::LongArray(Vec::new())),
        ("[B;1,2,]", Tag::ByteArray(vec![1, 2])),
        ("[B;1b,2b]", Tag::ByteArray(vec![1, 2])),
        ("[B;1,255ub]", Tag::ByteArray(vec![1, -1])),
        ("[B;-1]", Tag::ByteArray(vec![-1])),
        ("[B;0xFF]", Tag::ByteArray(vec![-1])),
        ("[I;1b,2s,3]", Tag::IntArray(vec![1, 2, 3])),
        ("[I;0x10]", Tag::IntArray(vec![16])),
        ("[I;0xFFFFFFFF]", Tag::IntArray(vec![-1])),
        ("[L;1b,2s,3i,4l]", Tag::LongArray(vec![1, 2, 3, 4])),
        ("[L;0xFFFFFFFFFFFFFFFF]", Tag::LongArray(vec![-1])),
        ("[ B ; 1 ]", Tag::ByteArray(vec![1])),
        ("[ I ; 1b ]", Tag::IntArray(vec![1])),
        ("[]", Tag::List(List::End)),
        ("[ ]", Tag::List(List::End)),
        ("[1]", Tag::List(List::Int(vec![1]))),
        ("[1,2,]", Tag::List(List::Int(vec![1, 2]))),
        ("[+1,-2]", Tag::List(List::Int(vec![1, -2]))),
        ("[true,false]", Tag::List(List::Byte(vec![1, 0]))),
        (
            "[[1],[2]]",
            Tag::List(List::List(vec![List::Int(vec![1]), List::Int(vec![2])])),
        ),
        (
            "[{a:1},{b:2}]",
            Tag::List(List::Compound(vec![
                compound(&[("a", Tag::Int(1))]),
                compound(&[("b", Tag::Int(2))]),
            ])),
        ),
        ("{}", Tag::Compound(Compound::new())),
        ("{a:1}", tag_compound(&[("a", Tag::Int(1))])),
        ("{ a : 1 }", tag_compound(&[("a", Tag::Int(1))])),
        ("{a:1,}", tag_compound(&[("a", Tag::Int(1))])),
        ("{a:1,a:2}", tag_compound(&[("a", Tag::Int(2))])),
        ("{1:2}", tag_compound(&[("1", Tag::Int(2))])),
        ("{-1:1}", tag_compound(&[("-1", Tag::Int(1))])),
        ("{.a:1}", tag_compound(&[(".a", Tag::Int(1))])),
        ("{\"a b\":1}", tag_compound(&[("a b", Tag::Int(1))])),
        ("{_ : 1}", tag_compound(&[("_", Tag::Int(1))])),
    ] {
        accept(input, expected);
    }
}

#[test]
fn rejects_malformed_numbers() {
    for input in [
        "1_",
        "1__b",
        "0x_1",
        "0x1_",
        "1_.0",
        "1._0",
        "1_e5",
        "1e_5",
        ".1_",
        "1e5_",
        "00",
        "01",
        "012",
        "0_0",
        "0x",
        "0X",
        "0b_1",
        "0b2",
        "0b102",
        "0xG",
        "0x1g",
        "1abc",
        "12abc",
        "-abc",
        "+abc",
        ".abc",
        "+bollocks",
        "1..2",
        "1.2.3",
        "1e",
        "1e+",
        "1e-",
        "--1",
        "+-1",
        "1-2",
        "0x-1",
        "0x+1",
        ".",
        "-.",
        "+.",
        "1.5b",
        "1.5s",
        "1.5i",
        "1.5l",
        "1e5b",
        "1uf",
        "1sf",
        "1ud",
        "1sd",
        "0b1f",
        "0b1d",
        "-0x1",
        "-0x1l",
        "-0x1i",
        "-0x1b",
        "-0x1s",
        "-0x1ub",
        "-0x1ui",
        "240sb",
        "128sb",
        "-129sb",
        "256ub",
        "-1ub",
        "65536us",
        "4294967296ui",
        "18446744073709551616ul",
        "2147483648",
        "-2147483649",
        "9223372036854775807",
        "0x100000000",
        "0xFFFFFFFFF",
        "0xFFFFFFFFFFFFFFFF",
        "0b111111111111111111111111111111111",
        "3.5e38f",
        "3.4028236e38f",
        "-3.5e38f",
        "1e309",
        "-1e309",
        "1e999",
    ] {
        reject(input);
    }
}

#[test]
fn rejects_malformed_strings_and_structures() {
    for input in [
        "\"\\q\"",
        "\"\\/\"",
        "\"\\0\"",
        "\"\\a\"",
        "\"\\v\"",
        "\"\\e\"",
        "\"\\ \"",
        "\"\\x4\"",
        "\"\\x4g\"",
        "\"\\u123\"",
        "\"\\u123g\"",
        "\"\\U00110000\"",
        "\"\\uD800\"",
        "\"\\uDC00\"",
        "\"\\uD800x\"",
        "\"\\U0000D800\"",
        "\"\\N{}\"",
        "\"\\N{Not A Name}\"",
        "\"unterminated",
        "\"abc",
        "'it''s'",
        "abc:",
        "abc$",
        "a b",
        "héllo",
        "[,]",
        "[1,,]",
        "[1 2]",
        "[1;2]",
        "[01]",
        "[B;1s]",
        "[B;1i]",
        "[B;1L]",
        "[B;1,255]",
        "[B;true]",
        "[B;,1]",
        "[B;1,,]",
        "[b;1]",
        "[i;1]",
        "[l;1]",
        "[I;1l]",
        "[I;1.5]",
        "[I;true]",
        "[L;1f]",
        "{,}",
        "{a:1,,}",
        "{a:1 b:2}",
        "{\"\":1}",
        "{\"\" : 1}",
        "{:1}",
        "{a:}",
        "{a}",
        "{a:1,garbage}",
        "{",
        "[",
        "{a:1",
        "[1,",
        "1,",
        "1 2",
        "[1,2,3]]",
        "{}{}",
        "foo(1)",
        "bool()",
        "bool(\"x\")",
        "bool(\"1\")",
        "bool([1])",
        "bool({})",
        "uuid()",
        "uuid(1)",
        "uuid(\"00112233445566778899aabbccddeeff\")",
        "TRUE(1)",
        "",
        " ",
        "\t",
    ] {
        reject(input);
    }
}

#[test]
fn errors_report_position_and_reason() {
    let error = snbt::parse("1_").unwrap_err();
    assert_eq!(error.position(), 0);
    assert!(error.message().contains("underscore"), "{error}");

    let error = snbt::parse("[1,2").unwrap_err();
    assert_eq!(error.position(), 4);
    assert!(error.message().contains("unterminated"), "{error}");

    let error = snbt::parse("{a:1").unwrap_err();
    assert_eq!(error.position(), 4);
    assert!(error.message().contains("unterminated"), "{error}");

    let error = snbt::parse("240sb").unwrap_err();
    assert!(error.message().contains("range"), "{error}");

    let error = snbt::parse("1e999").unwrap_err();
    assert!(error.message().contains("range"), "{error}");

    let error = snbt::parse("\"\\q\"").unwrap_err();
    assert!(error.message().contains("escape"), "{error}");

    let error = snbt::parse("1 trailing").unwrap_err();
    assert_eq!(error.position(), 2);
    assert!(error.to_string().contains("byte 2"), "{error}");
}

#[test]
fn heterogeneous_lists_use_vanilla_wrappers_and_roundtrip() {
    let input = r#"["a",{b:3},1]"#;
    let tag = snbt::parse(input).unwrap();
    let Tag::List(List::Compound(items)) = &tag else {
        panic!("expected wrapper compound list, got {tag:?}");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].get(""), Some(&Tag::String("a".into())));
    assert_eq!(items[1].get("b"), Some(&Tag::Int(3)));
    assert_eq!(items[1].len(), 1, "compound elements are not wrapped");
    assert_eq!(items[2].get(""), Some(&Tag::Int(1)));

    assert_eq!(snbt::format(&tag), input);
    assert_eq!(snbt::parse(&snbt::format(&tag)).unwrap(), tag);

    for input in [
        r#"[1b,2]"#,
        r#"[1,2.0]"#,
        r#"[[1],"a"]"#,
        r#"[{},[]]"#,
        r#"[true,"false"]"#,
        r#"[1,{b:2},"c",[3]]"#,
    ] {
        let tag = snbt::parse(input).unwrap();
        let formatted = snbt::format(&tag);
        assert_eq!(
            snbt::parse(&formatted).unwrap(),
            tag,
            "input {input:?} formatted as {formatted:?}"
        );
    }
}

#[test]
fn formats_every_tag_variant() {
    let tags = [
        Tag::Byte(-128),
        Tag::Byte(127),
        Tag::Short(-32768),
        Tag::Short(32767),
        Tag::Int(i32::MIN),
        Tag::Int(i32::MAX),
        Tag::Long(i64::MIN),
        Tag::Long(i64::MAX),
        Tag::Float(f32::MIN),
        Tag::Float(f32::MAX),
        Tag::Float(f32::MIN_POSITIVE),
        Tag::Float(-0.0),
        Tag::Float(f32::NAN),
        Tag::Float(f32::INFINITY),
        Tag::Float(f32::NEG_INFINITY),
        Tag::Double(f64::MIN),
        Tag::Double(f64::MAX),
        Tag::Double(f64::MIN_POSITIVE),
        Tag::Double(-0.0),
        Tag::Double(f64::NAN),
        Tag::Double(f64::INFINITY),
        Tag::Double(f64::NEG_INFINITY),
        Tag::ByteArray(Vec::new()),
        Tag::ByteArray(vec![i8::MIN, -1, 0, 1, i8::MAX]),
        Tag::IntArray(Vec::new()),
        Tag::IntArray(vec![i32::MIN, -1, 0, 1, i32::MAX]),
        Tag::LongArray(Vec::new()),
        Tag::LongArray(vec![i64::MIN, -1, 0, 1, i64::MAX]),
        Tag::String(String::new()),
        Tag::String("quotes \" ' backslash \\ newline \n null \0 snowman ☃".into()),
        Tag::List(List::End),
        Tag::List(List::Byte(vec![-1, 0, 1])),
        Tag::List(List::Short(vec![-1, 0, 1])),
        Tag::List(List::Int(vec![-1, 0, 1])),
        Tag::List(List::Long(vec![-1, 0, 1])),
        Tag::List(List::Float(vec![-1.0, 0.0, 1.5])),
        Tag::List(List::Double(vec![-1.0, 0.0, 1.5])),
        Tag::List(List::ByteArray(vec![vec![1, 2], vec![]])),
        Tag::List(List::String(vec!["a".into(), "b c".into()])),
        Tag::List(List::List(vec![
            List::End,
            List::Int(vec![1]),
            List::String(vec!["x".into()]),
        ])),
        Tag::List(List::Compound(vec![
            compound(&[("a", Tag::Int(1))]),
            compound(&[]),
        ])),
        Tag::List(List::IntArray(vec![vec![1, 2]])),
        Tag::List(List::LongArray(vec![vec![1, 2]])),
        tag_compound(&[
            ("byte", Tag::Byte(1)),
            ("short", Tag::Short(2)),
            ("int", Tag::Int(3)),
            ("long", Tag::Long(4)),
            ("float", Tag::Float(5.5)),
            ("double", Tag::Double(6.5)),
            ("string", Tag::String("seven".into())),
            ("list", Tag::List(List::End)),
            ("compound", Tag::Compound(Compound::new())),
            ("byte array", Tag::ByteArray(vec![1])),
            ("int array", Tag::IntArray(vec![1])),
            ("long array", Tag::LongArray(vec![1])),
        ]),
        Tag::Compound(Compound::new()),
    ];
    for tag in &tags {
        roundtrip(tag);
    }
}

/// Empty typed lists are the one shape SNBT cannot express; both vanilla and
/// Ferrite write them as `[]`, which parses as an empty end-typed list.
#[test]
fn empty_typed_lists_collapse_to_empty_end_list() {
    let typed_empty = Tag::List(List::String(Vec::new()));
    assert_eq!(snbt::format(&typed_empty), "[]");
    assert_eq!(snbt::parse("[]").unwrap(), Tag::List(List::End));
}

#[test]
fn property_roundtrip_random_tags() {
    for seed in 0..400 {
        let mut rng = Rng::new(seed);
        let tag = random_tag(&mut rng, 3);
        let text = snbt::format(&tag);
        let parsed = match snbt::parse(&text) {
            Ok(parsed) => parsed,
            Err(error) => panic!("seed {seed}: formatted {text:?} does not parse: {error}"),
        };
        assert!(
            same_tag(&tag, &parsed),
            "seed {seed}: {tag:?} formatted to {text:?} parsed as {parsed:?}"
        );
    }
}

#[test]
fn arbitrary_bytes_never_panic() {
    let mut rng = Rng::new(0xC0FF_EE12_3456_789A);
    for _ in 0..10_000 {
        let len = rng.below(48);
        let mut bytes = vec![0u8; len];
        for byte in &mut bytes {
            *byte = rng.next() as u8;
        }
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(tag) = snbt::parse(&text) {
            let formatted = snbt::format(&tag);
            let reparsed = snbt::parse(&formatted)
                .unwrap_or_else(|error| panic!("{text:?} -> {tag:?} -> {formatted:?}: {error}"));
            assert!(
                same_tag(&tag, &reparsed),
                "{text:?} -> {tag:?} -> {formatted:?} -> {reparsed:?}"
            );
        }
    }
}

#[test]
fn arbitrary_grammar_characters_never_panic() {
    const CHARS: &[char] = &[
        '{', '}', '[', ']', '(', ')', ',', ':', ';', '\'', '"', '\\', ' ', '\t', '\n', '\r', '0',
        '1', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'x', 'X', 'B', 'I', 'L', 'u', 'U', 'N', 's', 'S',
        '.', '+', '-', '_', '/', 'é', '☃', '😀',
    ];
    let mut rng = Rng::new(0x5EED_1234_5678_9ABC);
    for _ in 0..10_000 {
        let len = rng.below(64);
        let mut text = String::new();
        for _ in 0..len {
            text.push(CHARS[rng.below(CHARS.len())]);
        }
        if let Ok(tag) = snbt::parse(&text) {
            let formatted = snbt::format(&tag);
            assert!(
                snbt::parse(&formatted).is_ok(),
                "{text:?} -> {tag:?} -> {formatted:?} should parse"
            );
        }
    }
}

#[test]
fn excessive_nesting_is_rejected_without_stack_overflow() {
    let mut text = "[".repeat(600);
    text.push_str(&"]".repeat(600));
    let error = snbt::parse(&text).unwrap_err();
    assert!(error.message().contains("depth"), "{error}");

    let mut text = "{a:".repeat(600);
    text.push('1');
    text.push_str(&"}".repeat(600));
    assert!(snbt::parse(&text).is_err());
}

fn roundtrip(tag: &Tag) {
    let text = snbt::format(tag);
    let parsed = match snbt::parse(&text) {
        Ok(parsed) => parsed,
        Err(error) => panic!("{tag:?} formatted to {text:?} does not parse: {error}"),
    };
    assert!(
        same_tag(tag, &parsed),
        "{tag:?} formatted to {text:?} parsed as {parsed:?}"
    );
}

fn same_tag(a: &Tag, b: &Tag) -> bool {
    match (a, b) {
        (Tag::Byte(a), Tag::Byte(b)) => a == b,
        (Tag::Short(a), Tag::Short(b)) => a == b,
        (Tag::Int(a), Tag::Int(b)) => a == b,
        (Tag::Long(a), Tag::Long(b)) => a == b,
        (Tag::Float(a), Tag::Float(b)) => same_f32(*a, *b),
        (Tag::Double(a), Tag::Double(b)) => same_f64(*a, *b),
        (Tag::ByteArray(a), Tag::ByteArray(b)) => a == b,
        (Tag::String(a), Tag::String(b)) => a == b,
        (Tag::List(a), Tag::List(b)) => same_list(a, b),
        (Tag::Compound(a), Tag::Compound(b)) => same_compound(a, b),
        (Tag::IntArray(a), Tag::IntArray(b)) => a == b,
        (Tag::LongArray(a), Tag::LongArray(b)) => a == b,
        _ => false,
    }
}

fn same_f32(a: f32, b: f32) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

fn same_f64(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

fn same_list(a: &List, b: &List) -> bool {
    match (a, b) {
        (List::End, List::End) => true,
        (List::Byte(a), List::Byte(b)) => a == b,
        (List::Short(a), List::Short(b)) => a == b,
        (List::Int(a), List::Int(b)) => a == b,
        (List::Long(a), List::Long(b)) => a == b,
        (List::Float(a), List::Float(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_f32(*a, *b))
        }
        (List::Double(a), List::Double(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_f64(*a, *b))
        }
        (List::ByteArray(a), List::ByteArray(b)) => a == b,
        (List::String(a), List::String(b)) => a == b,
        (List::List(a), List::List(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_list(a, b))
        }
        (List::Compound(a), List::Compound(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_compound(a, b))
        }
        (List::IntArray(a), List::IntArray(b)) => a == b,
        (List::LongArray(a), List::LongArray(b)) => a == b,
        _ => false,
    }
}

fn same_compound(a: &Compound, b: &Compound) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|((a_key, a_value), (b_key, b_value))| {
                a_key == b_key && same_tag(a_value, b_value)
            })
}

/// Small deterministic splitmix64 PRNG; no external dependencies.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, limit: usize) -> usize {
        if limit == 0 {
            return 0;
        }
        (self.next() % limit as u64) as usize
    }
}

const STRING_CHARS: &[char] = &[
    'a', 'Z', '0', '"', '\'', '\\', '\n', '\t', '\r', '\u{8}', '\u{c}', '\0', '\u{7f}', '☃', '😀',
    ' ', '_', '-', '+', '.', '/', '$', ':', '{', '}', '[', ']',
];

const KEY_CHARS: &[char] = &[
    'a', 'Z', '0', '_', '-', '+', '.', ' ', '"', '\'', '\\', ':', '{', '}', '[', ']', '☃',
];

fn random_string(rng: &mut Rng) -> String {
    let len = rng.below(8);
    (0..len)
        .map(|_| STRING_CHARS[rng.below(STRING_CHARS.len())])
        .collect()
}

fn random_key(rng: &mut Rng) -> String {
    let len = 1 + rng.below(6);
    (0..len)
        .map(|_| KEY_CHARS[rng.below(KEY_CHARS.len())])
        .collect()
}

fn random_compound(rng: &mut Rng, depth: usize) -> Compound {
    let len = rng.below(4);
    let mut map = Compound::new();
    for index in 0..len {
        let mut key = random_key(rng);
        // Duplicate keys are legal in SNBT (the last value wins), but using
        // distinct keys keeps the generated tree easier to inspect.
        key.push(char::from(b'a' + index as u8));
        map.insert(key, random_tag(rng, depth));
    }
    map
}

fn random_byte_array(rng: &mut Rng) -> Vec<i8> {
    (0..rng.below(5)).map(|_| rng.next() as i8).collect()
}

fn random_int_array(rng: &mut Rng) -> Vec<i32> {
    (0..rng.below(5)).map(|_| rng.next() as i32).collect()
}

fn random_long_array(rng: &mut Rng) -> Vec<i64> {
    (0..rng.below(5)).map(|_| rng.next() as i64).collect()
}

fn random_tag(rng: &mut Rng, depth: usize) -> Tag {
    if depth == 0 {
        return random_leaf(rng);
    }
    match rng.below(12) {
        0..=5 => random_leaf(rng),
        6 => Tag::ByteArray(random_byte_array(rng)),
        7 => Tag::IntArray(random_int_array(rng)),
        8 => Tag::LongArray(random_long_array(rng)),
        9 | 10 => Tag::Compound(random_compound(rng, depth - 1)),
        _ => Tag::List(random_list(rng, depth - 1)),
    }
}

fn random_leaf(rng: &mut Rng) -> Tag {
    match rng.below(10) {
        0 => Tag::Byte(rng.next() as i8),
        1 => Tag::Short(rng.next() as i16),
        2 => Tag::Int(rng.next() as i32),
        3 => Tag::Long(rng.next() as i64),
        4 => Tag::Float(f32::from_bits(rng.next() as u32)),
        5 => Tag::Double(f64::from_bits(rng.next())),
        6 => Tag::String(random_string(rng)),
        7 => Tag::ByteArray(random_byte_array(rng)),
        8 => Tag::IntArray(random_int_array(rng)),
        _ => Tag::LongArray(random_long_array(rng)),
    }
}

fn random_list(rng: &mut Rng, depth: usize) -> List {
    if depth == 0 {
        return List::Int((0..1 + rng.below(4)).map(|_| rng.next() as i32).collect());
    }
    match rng.below(14) {
        0 => List::End,
        1 => List::Byte((0..1 + rng.below(3)).map(|_| rng.next() as i8).collect()),
        2 => List::Short((0..1 + rng.below(3)).map(|_| rng.next() as i16).collect()),
        3 => List::Int((0..1 + rng.below(3)).map(|_| rng.next() as i32).collect()),
        4 => List::Long((0..1 + rng.below(3)).map(|_| rng.next() as i64).collect()),
        5 => List::Float(
            (0..1 + rng.below(3))
                .map(|_| f32::from_bits(rng.next() as u32))
                .collect(),
        ),
        6 => List::Double(
            (0..1 + rng.below(3))
                .map(|_| f64::from_bits(rng.next()))
                .collect(),
        ),
        7 => List::ByteArray(
            (0..1 + rng.below(3))
                .map(|_| random_byte_array(rng))
                .collect(),
        ),
        8 => List::String((0..1 + rng.below(3)).map(|_| random_string(rng)).collect()),
        9 | 10 => List::List(
            (0..1 + rng.below(3))
                .map(|_| random_list(rng, depth - 1))
                .collect(),
        ),
        11 => List::Compound(
            (0..1 + rng.below(3))
                .map(|_| random_compound(rng, depth - 1))
                .collect(),
        ),
        12 => List::IntArray(
            (0..1 + rng.below(3))
                .map(|_| random_int_array(rng))
                .collect(),
        ),
        _ => {
            // A canonical heterogeneous list: non-compound elements wrapped
            // under the empty key, with a compound element left bare. The
            // formatter must unpack this without losing information.
            let first = random_non_compound_leaf(rng);
            let mut second = random_non_compound_leaf(rng);
            while second.id() == first.id() {
                second = random_non_compound_leaf(rng);
            }
            let mut values = vec![compound(&[("", first)]), compound(&[("", second)])];
            if rng.below(2) == 0 {
                values.push(random_compound(rng, depth - 1));
            }
            List::Compound(values)
        }
    }
}

fn random_non_compound_leaf(rng: &mut Rng) -> Tag {
    let mut tag = random_leaf(rng);
    while matches!(tag, Tag::Compound(_)) {
        tag = random_leaf(rng);
    }
    tag
}
