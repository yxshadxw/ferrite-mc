//! Textual NBT parser and formatter.
use crate::{Compound, List, Tag};
use std::fmt::Write;

#[derive(Debug, thiserror::Error)]
#[error("invalid SNBT at byte {0}: {1}")]
pub struct Error(usize, &'static str);
pub fn parse(input: &str) -> Result<Tag, Error> {
    let mut p = Parser { input, pos: 0 };
    let value = p.value(0)?;
    p.space();
    if p.pos != input.len() {
        return Err(Error(p.pos, "trailing input"));
    }
    Ok(value)
}
pub fn format(tag: &Tag) -> String {
    let mut out = String::new();
    format_tag(&mut out, tag);
    out
}
struct Parser<'a> {
    input: &'a str,
    pos: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }
    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn eat(&mut self, c: char) -> bool {
        self.space();
        if self.peek() == Some(c) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, c: char) -> Result<(), Error> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(Error(self.pos, "unexpected character"))
        }
    }
    fn string(&mut self) -> Result<String, Error> {
        self.space();
        if matches!(self.peek(), Some('\'' | '"')) {
            let quote = self.bump().unwrap();
            let mut s = String::new();
            loop {
                match self.bump() {
                    Some(c) if c == quote => break,
                    Some('\\') => match self.bump() {
                        Some(c) if c == quote || c == '\\' || c == '\'' || c == '"' => s.push(c),
                        Some('b') => s.push('\u{8}'),
                        Some('s') => s.push(' '),
                        Some('t') => s.push('\t'),
                        Some('n') => s.push('\n'),
                        Some('f') => s.push('\u{c}'),
                        Some('r') => s.push('\r'),
                        Some('N') => {
                            if self.bump() != Some('{') {
                                return Err(Error(self.pos, "invalid unicode name"));
                            }
                            let start = self.pos;
                            while self.peek().is_some_and(|c| c != '}') {
                                self.bump();
                            }
                            let name = &self.input[start..self.pos];
                            self.expect('}')?;
                            s.push(
                                unicode_names2::character(name)
                                    .ok_or(Error(self.pos, "unknown unicode name"))?,
                            );
                        }
                        Some(c @ ('x' | 'u' | 'U')) => {
                            let n = match c {
                                'x' => 2,
                                'u' => 4,
                                _ => 8,
                            };
                            let mut value = 0u32;
                            for _ in 0..n {
                                value = value * 16
                                    + self
                                        .bump()
                                        .and_then(|x| x.to_digit(16))
                                        .ok_or(Error(self.pos, "invalid unicode escape"))?;
                            }
                            if (0xD800..=0xDBFF).contains(&value) {
                                if self.bump() != Some('\\') || self.bump() != Some('u') {
                                    return Err(Error(self.pos, "missing low surrogate"));
                                }
                                let mut low = 0u32;
                                for _ in 0..4 {
                                    low = low * 16
                                        + self
                                            .bump()
                                            .and_then(|x| x.to_digit(16))
                                            .ok_or(Error(self.pos, "invalid low surrogate"))?;
                                }
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(Error(self.pos, "invalid low surrogate"));
                                }
                                value = 0x10000 + ((value - 0xD800) << 10) + (low - 0xDC00);
                            }
                            s.push(
                                char::from_u32(value)
                                    .ok_or(Error(self.pos, "invalid unicode scalar"))?,
                            );
                        }
                        _ => return Err(Error(self.pos, "invalid escape")),
                    },
                    Some(c) => s.push(c),
                    None => return Err(Error(self.pos, "unterminated string")),
                }
            }
            Ok(s)
        } else {
            let start = self.pos;
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '.'))
            {
                self.bump();
            }
            if start == self.pos {
                return Err(Error(self.pos, "expected string"));
            }
            Ok(self.input[start..self.pos].into())
        }
    }
    fn value(&mut self, depth: usize) -> Result<Tag, Error> {
        if depth > 512 {
            return Err(Error(self.pos, "depth limit"));
        }
        self.space();
        match self.peek() {
            Some('{') => {
                self.bump();
                let mut map = Compound::new();
                if self.eat('}') {
                    return Ok(Tag::Compound(map));
                }
                loop {
                    let key = self.string()?;
                    self.expect(':')?;
                    let v = self.value(depth + 1)?;
                    map.insert(key, v);
                    if self.eat('}') {
                        break;
                    }
                    self.expect(',')?;
                    if self.eat('}') {
                        break;
                    }
                }
                Ok(Tag::Compound(map))
            }
            Some('[') => self.list(depth + 1),
            Some('\'' | '"') => Ok(Tag::String(self.string()?)),
            Some(_) => {
                let s = self.string()?;
                if matches!(s.as_str(), "bool" | "uuid") && self.eat('(') {
                    let arg = self.value(depth + 1)?;
                    self.expect(')')?;
                    if s == "bool" {
                        let yes = match arg {
                            Tag::Byte(v) => v != 0,
                            Tag::Short(v) => v != 0,
                            Tag::Int(v) => v != 0,
                            Tag::Long(v) => v != 0,
                            Tag::Float(v) => v != 0.0,
                            Tag::Double(v) => v != 0.0,
                            _ => return Err(Error(self.pos, "bool expects number")),
                        };
                        Ok(Tag::Byte(i8::from(yes)))
                    } else if let Tag::String(v) = arg {
                        let id = uuid::Uuid::parse_str(&v)
                            .map_err(|_| Error(self.pos, "invalid UUID"))?;
                        Ok(Tag::IntArray(
                            id.as_bytes()
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .map(|x| i32::from_be_bytes(*x))
                                .collect(),
                        ))
                    } else {
                        Err(Error(self.pos, "uuid expects string"))
                    }
                } else {
                    parse_atom(s, None).map_err(|reason| Error(self.pos, reason))
                }
            }
            None => Err(Error(self.pos, "expected value")),
        }
    }
    fn list(&mut self, depth: usize) -> Result<Tag, Error> {
        self.expect('[')?;
        self.space();
        let array = if matches!(self.peek(), Some('B' | 'I' | 'L'))
            && self.input[self.pos + 1..].starts_with(';')
        {
            let c = self.bump().unwrap();
            self.bump();
            Some(c)
        } else {
            None
        };
        let mut values = Vec::new();
        if !self.eat(']') {
            loop {
                if let Some(a) = array {
                    self.space();
                    let start = self.pos;
                    let token = self.string()?;
                    values.push(parse_atom(token, Some(a)).map_err(|reason| Error(start, reason))?);
                } else {
                    values.push(self.value(depth)?);
                }
                if self.eat(']') {
                    break;
                }
                self.expect(',')?;
                if self.eat(']') {
                    break;
                }
            }
        }
        macro_rules! convert {
            ($variant:ident, $result:ident) => {{
                let mut out = Vec::with_capacity(values.len());
                for v in values {
                    if let Tag::$variant(x) = v {
                        out.push(x);
                    } else {
                        return Err(Error(self.pos, "mixed list types"));
                    }
                }
                List::$result(out)
            }};
        }
        if let Some(a) = array {
            return Ok(match a {
                'B' => {
                    let mut out = Vec::new();
                    for v in values {
                        if let Tag::Byte(x) = v {
                            out.push(x);
                        } else {
                            return Err(Error(self.pos, "invalid byte array"));
                        }
                    }
                    Tag::ByteArray(out)
                }
                'I' => {
                    let mut out = Vec::new();
                    for v in values {
                        match v {
                            Tag::Byte(x) => out.push(i32::from(x)),
                            Tag::Short(x) => out.push(i32::from(x)),
                            Tag::Int(x) => out.push(x),
                            _ => return Err(Error(self.pos, "invalid int array")),
                        }
                    }
                    Tag::IntArray(out)
                }
                _ => {
                    let mut out = Vec::new();
                    for v in values {
                        match v {
                            Tag::Byte(x) => out.push(i64::from(x)),
                            Tag::Short(x) => out.push(i64::from(x)),
                            Tag::Int(x) => out.push(i64::from(x)),
                            Tag::Long(x) => out.push(x),
                            _ => return Err(Error(self.pos, "invalid long array")),
                        }
                    }
                    Tag::LongArray(out)
                }
            });
        }
        if values.is_empty() {
            return Ok(Tag::List(List::End));
        }
        let id = values[0].id();
        if values.iter().any(|v| v.id() != id) {
            return Ok(Tag::List(List::Compound(
                values
                    .into_iter()
                    .map(|v| match v {
                        Tag::Compound(c) => c,
                        other => {
                            let mut c = Compound::new();
                            c.insert(String::new(), other);
                            c
                        }
                    })
                    .collect(),
            )));
        }
        Ok(Tag::List(match id {
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
            12 => convert!(LongArray, LongArray),
            _ => unreachable!(),
        }))
    }
}
fn parse_atom(s: String, default: Option<char>) -> Result<Tag, &'static str> {
    if s == "true" {
        return Ok(Tag::Byte(1));
    }
    if s == "false" {
        return Ok(Tag::Byte(0));
    }
    let numeric = s.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'));
    if !numeric {
        return if default.is_none() {
            Ok(Tag::String(s))
        } else {
            Err("expected number")
        };
    }
    let lower = s.to_ascii_lowercase();
    let unsigned_body = lower.trim_start_matches(['+', '-']);
    let radix = if unsigned_body.starts_with("0x") {
        16
    } else if unsigned_body.starts_with("0b") {
        2
    } else {
        10
    };
    let mut end = lower.len();
    let last = lower.as_bytes()[end - 1] as char;
    let has_suffix = matches!(last, 'b' | 's' | 'i' | 'l' | 'f' | 'd')
        && !(radix == 16
            && matches!(last, 'b' | 'd' | 'f')
            && !(lower.ends_with("ub") || lower.ends_with("sb")));
    let suffix = if has_suffix {
        end -= 1;
        last
    } else {
        default.unwrap_or('i').to_ascii_lowercase()
    };
    let explicit_sign = if has_suffix && end > 0 && matches!(lower.as_bytes()[end - 1], b'u' | b's')
    {
        end -= 1;
        Some(lower.as_bytes()[end] as char)
    } else {
        None
    };
    let body = &lower[..end];
    if body.starts_with('_')
        || body.ends_with('_')
        || body.contains("._")
        || body.contains("_.")
        || body.contains("e_")
        || body.contains("_e")
    {
        return Err("invalid digit separator");
    }
    if body.trim_start_matches(['+', '-']).starts_with("0x_")
        || body.trim_start_matches(['+', '-']).starts_with("0b_")
    {
        return Err("invalid digit separator");
    }
    let clean = body.replace('_', "");
    let is_float =
        radix == 10 && (matches!(suffix, 'f' | 'd') || clean.contains('.') || clean.contains('e'));
    if is_float {
        if explicit_sign.is_some() || !matches!(suffix, 'f' | 'd' | 'i') {
            return Err("invalid float suffix");
        }
        let x: f64 = clean.parse().map_err(|_| "invalid float")?;
        if !x.is_finite() {
            return Err("float out of range");
        }
        if suffix == 'f' {
            let x = x as f32;
            if !x.is_finite() {
                return Err("float out of range");
            }
            return Ok(Tag::Float(x));
        }
        return Ok(Tag::Double(x));
    }
    let digits = clean.trim_start_matches(['+', '-']);
    let digits = if radix != 10 {
        digits.get(2..).ok_or("invalid integer")?
    } else {
        digits
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || (radix == 10 && digits.len() > 1 && digits.starts_with('0'))
    {
        return Err("invalid integer");
    }
    let magnitude = u128::from_str_radix(digits, radix).map_err(|_| "invalid integer")?;
    let negative = clean.starts_with('-');
    let unsigned = explicit_sign == Some('u') || (explicit_sign.is_none() && radix != 10);
    let bits = match suffix {
        'b' => 8,
        's' => 16,
        'i' => 32,
        'l' => 64,
        _ => return Err("invalid integer suffix"),
    };
    if unsigned && negative {
        return Err("negative unsigned integer");
    }
    let max = if unsigned {
        (1u128 << bits) - 1
    } else if negative {
        1u128 << (bits - 1)
    } else {
        (1u128 << (bits - 1)) - 1
    };
    if magnitude > max {
        return Err("integer out of range");
    }
    let value = if negative {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };
    Ok(match suffix {
        'b' => Tag::Byte(value as i8),
        's' => Tag::Short(value as i16),
        'i' => Tag::Int(value as i32),
        _ => Tag::Long(value as i64),
    })
}

fn quoted(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
}
fn format_tag(out: &mut String, tag: &Tag) {
    macro_rules! number {
        ($v:expr, $s:literal) => {{
            let _ = write!(out, concat!("{}", $s), $v);
        }};
    }
    match tag {
        Tag::Byte(v) => number!(v, "b"),
        Tag::Short(v) => number!(v, "s"),
        Tag::Int(v) => number!(v, ""),
        Tag::Long(v) => number!(v, "L"),
        Tag::Float(v) => number!(v, "f"),
        Tag::Double(v) => number!(v, "d"),
        Tag::String(v) => quoted(out, v),
        Tag::Compound(v) => {
            out.push('{');
            for (i, (k, v)) in v.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                quoted(out, k);
                out.push(':');
                format_tag(out, v);
            }
            out.push('}');
        }
        Tag::ByteArray(v) => {
            out.push_str("[B;");
            for (i, x) in v.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                number!(x, "b");
            }
            out.push(']');
        }
        Tag::IntArray(v) => {
            out.push_str("[I;");
            for (i, x) in v.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                number!(x, "");
            }
            out.push(']');
        }
        Tag::LongArray(v) => {
            out.push_str("[L;");
            for (i, x) in v.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                number!(x, "L");
            }
            out.push(']');
        }
        Tag::List(v) => {
            out.push('[');
            macro_rules! elems {
                ($v:expr, $tag:ident) => {
                    for (i, x) in $v.iter().enumerate() {
                        if i != 0 {
                            out.push(',');
                        }
                        format_tag(out, &Tag::$tag(x.clone()));
                    }
                };
            }
            match v {
                List::End => {}
                List::Byte(v) => elems!(v, Byte),
                List::Short(v) => elems!(v, Short),
                List::Int(v) => elems!(v, Int),
                List::Long(v) => elems!(v, Long),
                List::Float(v) => elems!(v, Float),
                List::Double(v) => elems!(v, Double),
                List::ByteArray(v) => elems!(v, ByteArray),
                List::String(v) => elems!(v, String),
                List::List(v) => elems!(v, List),
                List::Compound(v) => elems!(v, Compound),
                List::IntArray(v) => elems!(v, IntArray),
                List::LongArray(v) => elems!(v, LongArray),
            }
            out.push(']');
        }
    }
}
