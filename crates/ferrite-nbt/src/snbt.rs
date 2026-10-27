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
                        Some(c) if c == quote || c == '\\' => s.push(c),
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
                }
                Ok(Tag::Compound(map))
            }
            Some('[') => self.list(depth + 1),
            Some('\'' | '"') => Ok(Tag::String(self.string()?)),
            Some(_) => {
                let s = self.string()?;
                Ok(parse_atom(s))
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
                values.push(self.value(depth)?);
                if self.eat(']') {
                    break;
                }
                self.expect(',')?;
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
                        if let Tag::Int(x) = v {
                            out.push(x);
                        } else {
                            return Err(Error(self.pos, "invalid int array"));
                        }
                    }
                    Tag::IntArray(out)
                }
                _ => {
                    let mut out = Vec::new();
                    for v in values {
                        if let Tag::Long(x) = v {
                            out.push(x);
                        } else {
                            return Err(Error(self.pos, "invalid long array"));
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
fn parse_atom(s: String) -> Tag {
    if s.eq_ignore_ascii_case("true") {
        return Tag::Byte(1);
    }
    if s.eq_ignore_ascii_case("false") {
        return Tag::Byte(0);
    }
    let suffix = s
        .as_bytes()
        .last()
        .copied()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let stem = &s[..s.len() - 1];
    macro_rules! number {
        ($t:ty, $v:ident) => {
            if let Ok(n) = stem.parse::<$t>() {
                return Tag::$v(n);
            }
        };
    }
    match suffix {
        b'b' => number!(i8, Byte),
        b's' => number!(i16, Short),
        b'l' => number!(i64, Long),
        b'f' => number!(f32, Float),
        b'd' => number!(f64, Double),
        _ => {}
    }
    if let Ok(n) = s.parse::<i32>() {
        return Tag::Int(n);
    }
    if (s.contains('.') || s.contains('e') || s.contains('E'))
        && let Ok(n) = s.parse::<f64>()
    {
        return Tag::Double(n);
    }
    Tag::String(s)
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
