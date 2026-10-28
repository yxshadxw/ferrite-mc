//! Textual NBT parser and formatter (SNBT).
//!
//! The grammar follows Minecraft 26.x SNBT (the 25w09a/1.21.5 grammar as it is
//! implemented by the 26.3 server's `SnbtGrammar`):
//!
//! * Values: [`Tag::Compound`], [`Tag::List`], typed arrays (`[B;]`, `[I;]`,
//!   `[L;]`), quoted strings (single or double quotes), unquoted strings,
//!   numbers, `true`/`false` (case-insensitive, stored as byte 0/1), and the
//!   `bool(...)` / `uuid("...")` conversion operations.
//! * Numbers may be decimal, hexadecimal (`0x`) or binary (`0b`); may use runs
//!   of `_` between digits; may carry a type suffix (`b`, `s`, `i`, `l`, `f`, `d`,
//!   case-insensitive) and an integer signedness prefix (`s` or `u`). A bare
//!   decimal integer defaults to int, a decimal with `.` or an exponent
//!   defaults to double, and hexadecimal/binary literals default to unsigned.
//!   Hexadecimal and binary numbers use the same default type as their suffix
//!   (int, or the enclosing array's element type).
//! * Lists and compounds allow exactly one trailing comma after a complete
//!   element.
//! * Heterogeneous lists are stored the way vanilla serializes them: as a
//!   compound list, with each non-compound element wrapped in a single-entry
//!   compound under the empty key (`["a", {b:3}]` becomes
//!   `[{"" : "a"}, {b:3}]` in the NBT model). [`format`] unpacks such wrappers
//!   again when it can do so unambiguously. Compound keys may not be empty.
//!
//! # Deliberate differences from vanilla
//!
//! * Non-finite floats are written as `NaNf`, `NaNd`, `Infinityf`,
//!   `Infinityd`, `-Infinityf` and `-Infinityd`, and are parsed back by this
//!   crate even though the vanilla parser treats `NaNf`/`Infinityf` as
//!   unquoted strings. This is what makes binary NBT containing non-finite
//!   values expressible as SNBT without losing the tag type.
//! * A lone UTF-16 surrogate escape such as `"\uD800"` is rejected because a
//!   Rust [`String`] cannot represent it; vanilla Java strings can.
//!
//! # Formatting round-trip
//!
//! [`format`] emits SNBT that [`parse`] reads back into an equal [`Tag`].
//! Equality is structural, with these caveats:
//!
//! * Empty typed lists (`List::String(vec![])`, `List::Compound(vec![])`, ...)
//!   have no SNBT spelling; they are written as `[]`, which parses as
//!   [`List::End`]. Vanilla behaves the same way.
//! * A compound whose key is empty cannot be written (vanilla rejects empty
//!   keys). Empty-key compounds only occur inside heterogeneous-list wrappers,
//!   which [`format`] unpacks whenever the result is still a heterogeneous
//!   list.
//! * `NaN` is canonicalized: its sign and payload bits are not preserved by
//!   the text form, but it is still a float/double tag after re-parsing.
//!   (`NaN` is never equal to itself under [`PartialEq`]; compare bits or use
//!   `is_nan()` when checking round-trips.) Negative zero is preserved
//!   (`-0.0f` / `-0.0d`).
//!
//! # Errors
//!
//! [`Error`] carries the byte offset at which parsing failed and a message.

use crate::{Compound, List, Tag};
use std::fmt::Write;

/// Maximum compound/list nesting accepted by the parser.
const MAX_DEPTH: usize = 512;

/// A parse error, with the byte offset in the input at which it occurred.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid SNBT at byte {position}: {message}")]
pub struct Error {
    position: usize,
    message: String,
}

impl Error {
    fn new(position: usize, message: impl Into<String>) -> Self {
        Self {
            position,
            message: message.into(),
        }
    }

    /// The byte offset into the input where parsing failed.
    pub fn position(&self) -> usize {
        self.position
    }

    /// The human-readable reason parsing failed.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Parses one complete SNBT value.
///
/// Whitespace around the value is ignored; anything other than whitespace
/// after it is an error.
pub fn parse(input: &str) -> Result<Tag, Error> {
    let mut parser = Parser { input, pos: 0 };
    let value = parser.value(0)?;
    parser.space();
    if parser.pos != input.len() {
        return Err(parser.error("unexpected trailing data"));
    }
    Ok(value)
}

/// Formats a tag as SNBT. The output parses back to an equal tag.
pub fn format(tag: &Tag) -> String {
    let mut out = String::new();
    format_tag(&mut out, tag);
    out
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn error(&self, message: impl Into<String>) -> Error {
        Error::new(self.pos, message)
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), Error> {
        self.space();
        match self.peek() {
            Some(c) if c == expected => {
                self.bump();
                Ok(())
            }
            Some(c) => Err(self.error(format!("expected `{expected}`, found `{c}`"))),
            None => Err(self.error(format!("expected `{expected}`, found end of input"))),
        }
    }

    /// Reads an unquoted token. The caller is positioned at its first character.
    fn token(&mut self) -> Result<&'a str, Error> {
        let start = self.pos;
        while self.peek().is_some_and(is_unquoted_char) {
            self.bump();
        }
        if start == self.pos {
            return Err(self.error("expected a value"));
        }
        Ok(&self.input[start..self.pos])
    }

    fn value(&mut self, depth: usize) -> Result<Tag, Error> {
        self.space();
        if depth > MAX_DEPTH {
            return Err(self.error("maximum nesting depth exceeded"));
        }
        match self.peek() {
            Some('{') => self.compound(depth),
            Some('[') => self.list(depth),
            Some('\'' | '"') => Ok(Tag::String(self.quoted()?)),
            Some(c) if is_unquoted_char(c) => self.unquoted_value(depth),
            Some(c) => Err(self.error(format!("unexpected character `{c}`"))),
            None => Err(self.error("expected a value")),
        }
    }

    fn compound(&mut self, depth: usize) -> Result<Tag, Error> {
        self.bump(); // '{'
        let mut map = Compound::new();
        self.space();
        if self.peek() == Some('}') {
            self.bump();
            return Ok(Tag::Compound(map));
        }
        loop {
            let key = self.key()?;
            self.expect(':')?;
            let value = self.value(depth + 1)?;
            map.insert(key, value);
            self.space();
            match self.bump() {
                Some('}') => return Ok(Tag::Compound(map)),
                Some(',') => {
                    self.space();
                    if self.peek() == Some('}') {
                        self.bump();
                        return Ok(Tag::Compound(map));
                    }
                }
                Some(c) => {
                    return Err(Error::new(
                        self.pos - c.len_utf8(),
                        format!("expected `,` or `}}`, found `{c}`"),
                    ));
                }
                None => return Err(self.error("unterminated compound")),
            }
        }
    }

    fn key(&mut self) -> Result<String, Error> {
        self.space();
        match self.peek() {
            Some('\'' | '"') => {
                let start = self.pos;
                let key = self.quoted()?;
                if key.is_empty() {
                    return Err(Error::new(start, "compound keys cannot be empty"));
                }
                Ok(key)
            }
            Some(c) if is_unquoted_char(c) => Ok(self.token()?.to_owned()),
            _ => Err(self.error("expected a compound key")),
        }
    }

    fn list(&mut self, depth: usize) -> Result<Tag, Error> {
        self.bump(); // '['
        self.space();
        if let Some(prefix) = self.array_prefix() {
            return self.array(prefix);
        }
        if self.peek() == Some(']') {
            self.bump();
            return Ok(Tag::List(List::End));
        }
        let mut values = Vec::new();
        loop {
            values.push(self.value(depth + 1)?);
            self.space();
            match self.bump() {
                Some(']') => break,
                Some(',') => {
                    self.space();
                    if self.peek() == Some(']') {
                        self.bump();
                        break;
                    }
                }
                Some(c) => {
                    return Err(Error::new(
                        self.pos - c.len_utf8(),
                        format!("expected `,` or `]`, found `{c}`"),
                    ));
                }
                None => return Err(self.error("unterminated list")),
            }
        }
        Ok(list_from_values(values))
    }

    /// Recognizes `B;`, `I;` or `L;` (whitespace allowed before the semicolon)
    /// after the opening bracket.
    fn array_prefix(&mut self) -> Option<char> {
        let save = self.pos;
        let Some(prefix @ ('B' | 'I' | 'L')) = self.peek() else {
            return None;
        };
        self.bump();
        self.space();
        if self.peek() == Some(';') {
            self.bump();
            Some(prefix)
        } else {
            self.pos = save;
            None
        }
    }

    fn array(&mut self, prefix: char) -> Result<Tag, Error> {
        let position = self.pos;
        let default = Some(prefix.to_ascii_lowercase());
        self.space();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(empty_array(prefix));
        }
        let mut values = Vec::new();
        loop {
            self.space();
            let start = self.pos;
            let token = self.token()?;
            values.push(parse_number(token, start, default)?);
            self.space();
            match self.bump() {
                Some(']') => break,
                Some(',') => {
                    self.space();
                    if self.peek() == Some(']') {
                        self.bump();
                        break;
                    }
                }
                Some(c) => {
                    return Err(Error::new(
                        self.pos - c.len_utf8(),
                        format!("expected `,` or `]`, found `{c}`"),
                    ));
                }
                None => return Err(self.error("unterminated array")),
            }
        }
        build_array(prefix, values, position)
    }

    fn unquoted_value(&mut self, depth: usize) -> Result<Tag, Error> {
        let start = self.pos;
        let token = self.token()?;
        if self.peek() == Some('(') {
            return self.operation(token, depth);
        }
        if token.eq_ignore_ascii_case("true") {
            return Ok(Tag::Byte(1));
        }
        if token.eq_ignore_ascii_case("false") {
            return Ok(Tag::Byte(0));
        }
        if let Some(tag) = non_finite(token) {
            return Ok(tag);
        }
        if starts_number(token) {
            return parse_number(token, start, None);
        }
        Ok(Tag::String(token.to_owned()))
    }

    fn operation(&mut self, name: &str, depth: usize) -> Result<Tag, Error> {
        self.bump(); // '('
        match name {
            "bool" => {
                let value = self.value(depth + 1)?;
                self.expect(')')?;
                let truthy = match value {
                    Tag::Byte(v) => v != 0,
                    Tag::Short(v) => v != 0,
                    Tag::Int(v) => v != 0,
                    Tag::Long(v) => v != 0,
                    Tag::Float(v) => v != 0.0,
                    Tag::Double(v) => v != 0.0,
                    _ => return Err(self.error("bool expects a number or a boolean")),
                };
                Ok(Tag::Byte(i8::from(truthy)))
            }
            "uuid" => {
                let value = self.value(depth + 1)?;
                self.expect(')')?;
                let Tag::String(text) = value else {
                    return Err(self.error("uuid expects a string"));
                };
                let Some(bits) = parse_uuid(&text) else {
                    return Err(self.error("invalid UUID string"));
                };
                Ok(Tag::IntArray(bits.to_vec()))
            }
            _ => Err(self.error(format!("no such operation `{name}`"))),
        }
    }

    fn quoted(&mut self) -> Result<String, Error> {
        let Some(quote @ ('\'' | '"')) = self.bump() else {
            return Err(self.error("expected a quoted string"));
        };
        let mut out = String::new();
        loop {
            let Some(c) = self.bump() else {
                return Err(self.error("unterminated string"));
            };
            if c == quote {
                return Ok(out);
            }
            if c != '\\' {
                out.push(c);
                continue;
            }
            let Some(escape) = self.bump() else {
                return Err(self.error("unterminated escape sequence"));
            };
            match escape {
                '\\' => out.push('\\'),
                '\'' => out.push('\''),
                '"' => out.push('"'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                's' => out.push(' '),
                't' => out.push('\t'),
                'x' => {
                    let value = self.hex_digits(2)?;
                    out.push(char::from_u32(value).expect("two hex digits fit a char"));
                }
                'u' => {
                    let mut value = self.hex_digits(4)?;
                    if (0xD800..=0xDBFF).contains(&value) {
                        if self.bump() != Some('\\') || self.bump() != Some('u') {
                            return Err(self.error("unpaired high surrogate"));
                        }
                        let low = self.hex_digits(4)?;
                        if !(0xDC00..=0xDFFF).contains(&low) {
                            return Err(self.error("invalid low surrogate"));
                        }
                        value = 0x10000 + ((value - 0xD800) << 10) + (low - 0xDC00);
                    } else if (0xDC00..=0xDFFF).contains(&value) {
                        return Err(self.error("unpaired low surrogate"));
                    }
                    out.push(
                        char::from_u32(value)
                            .ok_or_else(|| self.error("invalid unicode scalar value"))?,
                    );
                }
                'U' => {
                    let value = self.hex_digits(8)?;
                    out.push(
                        char::from_u32(value)
                            .ok_or_else(|| self.error("invalid unicode scalar value"))?,
                    );
                }
                'N' => {
                    if self.bump() != Some('{') {
                        return Err(self.error("expected `{` after `\\N`"));
                    }
                    let start = self.pos;
                    let mut name = String::new();
                    loop {
                        let Some(c) = self.bump() else {
                            return Err(self.error("unterminated unicode character name"));
                        };
                        if c == '}' {
                            break;
                        }
                        name.push(c);
                    }
                    let name = name.trim();
                    if name.is_empty() {
                        return Err(Error::new(start, "empty unicode character name"));
                    }
                    out.push(unicode_names2::character(name).ok_or_else(|| {
                        Error::new(start, format!("unknown unicode character name `{name}`"))
                    })?);
                }
                other => {
                    return Err(self.error(format!("invalid escape sequence `\\{other}`")));
                }
            }
        }
    }

    fn hex_digits(&mut self, count: usize) -> Result<u32, Error> {
        let mut value = 0u32;
        for _ in 0..count {
            let Some(c) = self.bump() else {
                return Err(self.error(format!("expected {count} hexadecimal digits")));
            };
            let Some(digit) = c.to_digit(16) else {
                return Err(self.error("invalid hexadecimal digit"));
            };
            value = value * 16 + digit;
        }
        Ok(value)
    }
}

/// Parses a UUID the way Java's `UUID.fromString` does (five hyphen-separated
/// hexadecimal groups, shifted together), returning the four int words used by
/// vanilla's `uuid(...)` operation.
fn parse_uuid(text: &str) -> Option<[i32; 4]> {
    let mut parts = text.split('-');
    let mut next = || {
        let part = parts.next()?;
        if part.is_empty() || part.len() > 16 {
            return None;
        }
        i64::from_str_radix(part, 16).ok()
    };
    let first = next()?;
    let second = next()?;
    let third = next()?;
    let fourth = next()?;
    let fifth = next()?;
    if parts.next().is_some() {
        return None;
    }
    let most = (first << 16 | second) << 16 | third;
    let least = fourth << 48 | fifth;
    Some([
        (most >> 32) as i32,
        most as i32,
        (least >> 32) as i32,
        least as i32,
    ])
}

fn is_unquoted_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '.')
}

fn starts_number(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.'))
}

/// Non-finite floats are a deliberate extension so that binary NBT values
/// survive a format/parse round-trip. Vanilla writes the same spelling but
/// cannot read it back.
fn non_finite(token: &str) -> Option<Tag> {
    Some(match token {
        "NaNf" => Tag::Float(f32::NAN),
        "NaNd" => Tag::Double(f64::NAN),
        "Infinityf" | "+Infinityf" => Tag::Float(f32::INFINITY),
        "Infinityd" | "+Infinityd" => Tag::Double(f64::INFINITY),
        "-Infinityf" => Tag::Float(f32::NEG_INFINITY),
        "-Infinityd" => Tag::Double(f64::NEG_INFINITY),
        _ => return None,
    })
}

fn empty_array(prefix: char) -> Tag {
    match prefix {
        'B' => Tag::ByteArray(Vec::new()),
        'I' => Tag::IntArray(Vec::new()),
        _ => Tag::LongArray(Vec::new()),
    }
}

fn build_array(prefix: char, values: Vec<Tag>, position: usize) -> Result<Tag, Error> {
    let invalid = || Error::new(position, "invalid array element type");
    Ok(match prefix {
        'B' => {
            let mut out = Vec::with_capacity(values.len());
            for value in values {
                match value {
                    Tag::Byte(v) => out.push(v),
                    _ => return Err(invalid()),
                }
            }
            Tag::ByteArray(out)
        }
        'I' => {
            let mut out = Vec::with_capacity(values.len());
            for value in values {
                match value {
                    Tag::Byte(v) => out.push(i32::from(v)),
                    Tag::Short(v) => out.push(i32::from(v)),
                    Tag::Int(v) => out.push(v),
                    _ => return Err(invalid()),
                }
            }
            Tag::IntArray(out)
        }
        _ => {
            let mut out = Vec::with_capacity(values.len());
            for value in values {
                match value {
                    Tag::Byte(v) => out.push(i64::from(v)),
                    Tag::Short(v) => out.push(i64::from(v)),
                    Tag::Int(v) => out.push(i64::from(v)),
                    Tag::Long(v) => out.push(v),
                    _ => return Err(invalid()),
                }
            }
            Tag::LongArray(out)
        }
    })
}

fn list_from_values(values: Vec<Tag>) -> Tag {
    debug_assert!(!values.is_empty());
    let id = values[0].id();
    if values.iter().all(|value| value.id() == id) {
        macro_rules! convert {
            ($variant:ident, $list:ident) => {{
                let mut out = Vec::with_capacity(values.len());
                for value in values {
                    let Tag::$variant(v) = value else {
                        unreachable!("all list elements share the first element's id");
                    };
                    out.push(v);
                }
                List::$list(out)
            }};
        }
        Tag::List(match id {
            1 => convert!(Byte, Byte),
            2 => convert!(Short, Short),
            3 => convert!(Int, Int),
            4 => convert!(Long, Long),
            5 => convert!(Float, Float),
            6 => convert!(Double, Double),
            7 => convert!(ByteArray, ByteArray),
            8 => convert!(String, String),
            9 => convert!(List, List),
            10 => convert!(Compound, Compound),
            11 => convert!(IntArray, IntArray),
            _ => convert!(LongArray, LongArray),
        })
    } else {
        // Vanilla serializes heterogeneous lists as compound lists, wrapping
        // every non-compound element under the empty key.
        Tag::List(List::Compound(
            values
                .into_iter()
                .map(|value| match value {
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
}

/// Parses a numeric token. `default` is the enclosing array's element type
/// (`b`, `i` or `l`) or `None` for a standalone value.
fn parse_number(token: &str, start: usize, default: Option<char>) -> Result<Tag, Error> {
    if let Some(tag) = non_finite(token) {
        return Ok(tag);
    }
    let lower = token.to_ascii_lowercase();
    let (negative, body) = match lower.as_bytes().first() {
        Some(b'+') => (false, &lower[1..]),
        Some(b'-') => (true, &lower[1..]),
        _ => (false, lower.as_str()),
    };
    if body.is_empty() {
        return Err(Error::new(start, "expected a number"));
    }

    let (radix, digits) = if let Some(digits) = body.strip_prefix("0x") {
        (16, digits)
    } else if let Some(digits) = body.strip_prefix("0b") {
        (2, digits)
    } else {
        (10, body)
    };

    // Vanilla backtracks `0b`/`0B` with no binary digits into the decimal
    // literal `0` plus the byte suffix `b`.
    if radix == 2 && digits.is_empty() && !negative {
        return Ok(Tag::Byte(0));
    }
    let digit_fn: fn(char) -> bool = match radix {
        16 => |c| c.is_ascii_hexdigit(),
        2 => |c| matches!(c, '0' | '1'),
        _ => |c| c.is_ascii_digit(),
    };

    let (suffix, signed_prefix, explicit_suffix, digits) = if radix == 16 {
        take_hex_suffix(digits, default)
    } else {
        take_suffix(digits, default)
    };
    check_underscores(digits, digit_fn, start)?;

    if radix != 10 && negative && signed_prefix.is_none() {
        return Err(Error::new(
            start,
            "negative hexadecimal and binary numbers need an explicit signed suffix",
        ));
    }
    if negative && signed_prefix == Some('u') {
        return Err(Error::new(start, "expected a non-negative number"));
    }
    let unsigned = signed_prefix == Some('u') || (signed_prefix.is_none() && radix != 10);

    let is_float = radix == 10
        && ((explicit_suffix && matches!(suffix, 'f' | 'd'))
            || (!explicit_suffix && digits.contains(['.', 'e'])));
    if is_float {
        check_decimal_float(digits, start)?;
        let clean = digits.replace('_', "");
        let text = if negative { format!("-{clean}") } else { clean };
        if suffix == 'f' {
            let value: f32 = text
                .parse()
                .map_err(|_| Error::new(start, "invalid float"))?;
            if !value.is_finite() {
                return Err(Error::new(start, "float is out of range"));
            }
            return Ok(Tag::Float(value));
        }
        let value: f64 = text
            .parse()
            .map_err(|_| Error::new(start, "invalid float"))?;
        if !value.is_finite() {
            return Err(Error::new(start, "float is out of range"));
        }
        return Ok(Tag::Double(value));
    }

    if radix != 10 && matches!(suffix, 'f' | 'd') {
        return Err(Error::new(
            start,
            "float suffixes are not allowed on hexadecimal or binary numbers",
        ));
    }
    let clean = digits.replace('_', "");
    if clean.is_empty() {
        return Err(Error::new(start, "expected an integer"));
    }
    if !clean.chars().all(digit_fn) {
        return Err(Error::new(start, "invalid digit in integer"));
    }
    if radix == 10 && clean.len() > 1 && clean.starts_with('0') {
        return Err(Error::new(
            start,
            "decimal integers cannot have leading zeroes",
        ));
    }
    let magnitude = u128::from_str_radix(&clean, radix)
        .map_err(|_| Error::new(start, "integer is too large"))?;
    let bits = match suffix {
        'b' => 8,
        's' => 16,
        'i' => 32,
        _ => 64,
    };
    let value = if unsigned {
        let max = (1u128 << bits) - 1;
        if magnitude > max {
            return Err(Error::new(start, "unsigned integer is out of range"));
        }
        magnitude
    } else if negative {
        let max = 1u128 << (bits - 1);
        if magnitude > max {
            return Err(Error::new(start, "signed integer is out of range"));
        }
        0u128.wrapping_sub(magnitude)
    } else {
        let max = (1u128 << (bits - 1)) - 1;
        if magnitude > max {
            return Err(Error::new(start, "signed integer is out of range"));
        }
        magnitude
    };
    let value = value as i128;
    Ok(match suffix {
        'b' => Tag::Byte(value as i8),
        's' => Tag::Short(value as i16),
        'i' => Tag::Int(value as i32),
        _ => Tag::Long(value as i64),
    })
}

/// Splits an optional signedness prefix and type suffix from a decimal or
/// binary token. Hexadecimal tokens need [`take_hex_suffix`] because `b`, `d`
/// and `f` are valid hex digits.
fn take_suffix(token: &str, default: Option<char>) -> (char, Option<char>, bool, &str) {
    let bytes = token.as_bytes();
    let len = bytes.len();
    if len >= 2 {
        let last = bytes[len - 1];
        let previous = bytes[len - 2];
        if matches!(last, b'b' | b's' | b'i' | b'l') && matches!(previous, b's' | b'u') {
            return (
                last as char,
                Some(previous as char),
                true,
                &token[..len - 2],
            );
        }
    }
    if let Some(&last) = bytes.last()
        && matches!(last, b'b' | b's' | b'i' | b'l' | b'f' | b'd')
    {
        return (last as char, None, true, &token[..len - 1]);
    }
    (default.unwrap_or('i'), None, false, token)
}

fn take_hex_suffix(token: &str, default: Option<char>) -> (char, Option<char>, bool, &str) {
    let bytes = token.as_bytes();
    let len = bytes.len();
    if len >= 2 && bytes[len - 1] == b'b' && matches!(bytes[len - 2], b's' | b'u') {
        return ('b', Some(bytes[len - 2] as char), true, &token[..len - 2]);
    }
    if let Some(&last) = bytes.last()
        && matches!(last, b's' | b'i' | b'l')
    {
        let mut end = len - 1;
        let mut signed = None;
        if end > 0 && matches!(bytes[end - 1], b's' | b'u') {
            signed = Some(bytes[end - 1] as char);
            end -= 1;
        }
        return (last as char, signed, true, &token[..end]);
    }
    (default.unwrap_or('i'), None, false, token)
}

fn check_underscores(digits: &str, is_digit: fn(char) -> bool, start: usize) -> Result<(), Error> {
    let chars: Vec<char> = digits.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '_' {
            index += 1;
            continue;
        }
        // Vanilla allows a whole run of underscores, as long as the run sits
        // between two digits (`1__2` is 12, `1_` and `1._5` are not numbers).
        let run = chars[index..].iter().take_while(|&&c| c == '_').count();
        let previous = index.checked_sub(1).and_then(|i| chars.get(i));
        let next = chars.get(index + run);
        if !previous.is_some_and(|&c| is_digit(c)) || !next.is_some_and(|&c| is_digit(c)) {
            return Err(Error::new(
                start,
                "underscores are only allowed between digits",
            ));
        }
        index += run;
    }
    Ok(())
}

fn check_decimal_float(digits: &str, start: usize) -> Result<(), Error> {
    let mut pieces = digits.split('e');
    let mantissa = pieces.next().unwrap_or_default();
    let exponent = pieces.next();
    if pieces.next().is_some() {
        return Err(Error::new(start, "invalid exponent"));
    }

    let mut mantissa_digits = 0;
    let mut points = 0;
    for c in mantissa.chars() {
        match c {
            '0'..='9' => mantissa_digits += 1,
            '.' => {
                points += 1;
                if points > 1 {
                    return Err(Error::new(start, "invalid decimal point"));
                }
            }
            '_' => {}
            _ => return Err(Error::new(start, "invalid character in float")),
        }
    }
    if mantissa_digits == 0 {
        return Err(Error::new(start, "expected a digit before the exponent"));
    }

    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        let exponent_digits = exponent.chars().filter(|c| c.is_ascii_digit()).count();
        if exponent_digits == 0 || !exponent.chars().all(|c| c.is_ascii_digit() || c == '_') {
            return Err(Error::new(start, "invalid exponent"));
        }
    }
    Ok(())
}

fn format_tag(out: &mut String, tag: &Tag) {
    match tag {
        Tag::Byte(v) => {
            let _ = write!(out, "{v}b");
        }
        Tag::Short(v) => {
            let _ = write!(out, "{v}s");
        }
        Tag::Int(v) => {
            let _ = write!(out, "{v}");
        }
        Tag::Long(v) => {
            let _ = write!(out, "{v}L");
        }
        Tag::Float(v) => format_float(out, *v),
        Tag::Double(v) => format_double(out, *v),
        Tag::String(v) => format_quoted(out, v),
        Tag::Compound(v) => format_compound(out, v),
        Tag::ByteArray(v) => {
            out.push_str("[B;");
            for (index, value) in v.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                let _ = write!(out, "{value}b");
            }
            out.push(']');
        }
        Tag::IntArray(v) => {
            out.push_str("[I;");
            for (index, value) in v.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                let _ = write!(out, "{value}");
            }
            out.push(']');
        }
        Tag::LongArray(v) => {
            out.push_str("[L;");
            for (index, value) in v.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                let _ = write!(out, "{value}L");
            }
            out.push(']');
        }
        Tag::List(list) => format_list(out, list),
    }
}

fn format_float(out: &mut String, value: f32) {
    if value.is_nan() {
        out.push_str("NaNf");
    } else if value == f32::INFINITY {
        out.push_str("Infinityf");
    } else if value == f32::NEG_INFINITY {
        out.push_str("-Infinityf");
    } else {
        let _ = write!(out, "{value:?}f");
    }
}

fn format_double(out: &mut String, value: f64) {
    if value.is_nan() {
        out.push_str("NaNd");
    } else if value == f64::INFINITY {
        out.push_str("Infinityd");
    } else if value == f64::NEG_INFINITY {
        out.push_str("-Infinityd");
    } else {
        let _ = write!(out, "{value:?}d");
    }
}

fn format_quoted(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn format_key(out: &mut String, key: &str) {
    if !key.is_empty() && key.chars().all(is_unquoted_char) {
        out.push_str(key);
    } else {
        format_quoted(out, key);
    }
}

fn format_compound(out: &mut String, compound: &Compound) {
    out.push('{');
    for (index, (key, value)) in compound.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        format_key(out, key);
        out.push(':');
        format_tag(out, value);
    }
    out.push('}');
}

/// A heterogeneous list is stored as a compound list whose non-compound
/// elements are wrapped under the empty key. When every wrapped value is a
/// non-compound tag and the recovered elements have more than one tag id, the
/// wrappers are unambiguous and are expanded back to the heterogeneous form.
fn format_list(out: &mut String, list: &List) {
    out.push('[');
    macro_rules! elements {
        ($values:expr, $tag:ident) => {
            for (index, value) in $values.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                format_tag(out, &Tag::$tag(value.clone()));
            }
        };
    }
    match list {
        List::End => {}
        List::Byte(values) => elements!(values, Byte),
        List::Short(values) => elements!(values, Short),
        List::Int(values) => elements!(values, Int),
        List::Long(values) => elements!(values, Long),
        List::Float(values) => elements!(values, Float),
        List::Double(values) => elements!(values, Double),
        List::ByteArray(values) => elements!(values, ByteArray),
        List::String(values) => elements!(values, String),
        List::List(values) => elements!(values, List),
        List::IntArray(values) => elements!(values, IntArray),
        List::LongArray(values) => elements!(values, LongArray),
        List::Compound(values) => {
            let unwrapped: Vec<Option<&Tag>> = values.iter().map(unwrap_element).collect();
            // 10 is the NBT compound tag id; a bare compound counts as one.
            let ids: Vec<u8> = unwrapped
                .iter()
                .map(|inner| inner.map_or(10, Tag::id))
                .collect();
            let recoverable = ids.windows(2).any(|pair| pair[0] != pair[1]);
            for (index, compound) in values.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                if recoverable && let Some(inner) = unwrapped[index] {
                    format_tag(out, inner);
                    continue;
                }
                format_compound(out, compound);
            }
        }
    }
    out.push(']');
}

fn unwrap_element(compound: &Compound) -> Option<&Tag> {
    if compound.len() != 1 {
        return None;
    }
    let (key, value) = compound.iter().next()?;
    (key.is_empty() && !matches!(value, Tag::Compound(_))).then_some(value)
}
