//! Lexical helpers shared by the Minecraft argument parsers.

use crate::error::{CommandSyntaxError, builtin, minecraft};
use crate::reader::{ARGUMENT_SEPARATOR, StringReader};

/// Characters vanilla allows in an identifier (`Identifier.isAllowedInIdentifier`).
pub fn is_allowed_in_identifier(c: char) -> bool {
    matches!(c, '0'..='9' | 'a'..='z' | '_' | ':' | '/' | '.' | '-')
}

fn valid_namespace(s: &str) -> bool {
    s.chars()
        .all(|c| matches!(c, '0'..='9' | 'a'..='z' | '_' | '.' | '-'))
}

fn valid_path(s: &str) -> bool {
    s.chars()
        .all(|c| matches!(c, '0'..='9' | 'a'..='z' | '_' | '.' | '-' | '/'))
}

/// Normalizes `ns:path` (default namespace `minecraft`), or `None` if it is not valid.
pub fn normalize_identifier(raw: &str) -> Option<String> {
    let (namespace, path) = match raw.split_once(':') {
        Some(("", path)) => ("minecraft", path),
        Some((ns, path)) => (ns, path),
        None => ("minecraft", raw),
    };
    (valid_namespace(namespace) && valid_path(path)).then(|| format!("{namespace}:{path}"))
}

/// Vanilla `Identifier.read`: reads identifier characters, then validates them.
pub fn read_identifier(reader: &mut StringReader) -> Result<String, CommandSyntaxError> {
    let start = reader.cursor();
    while reader.peek().is_some_and(is_allowed_in_identifier) {
        reader.skip();
    }
    let raw = &reader.string()[start..reader.cursor()];
    normalize_identifier(raw).ok_or_else(|| {
        reader.set_cursor(start);
        minecraft::id_invalid(reader)
    })
}

fn closing(open: char) -> Option<char> {
    match open {
        '{' => Some('}'),
        '[' => Some(']'),
        '(' => Some(')'),
        _ => None,
    }
}

/// Consumes one balanced `{...}`, `[...]` or `(...)` group, honouring quoted strings.
pub fn read_balanced(reader: &mut StringReader) -> Result<(), CommandSyntaxError> {
    let mut stack = Vec::new();
    loop {
        let Some(c) = reader.peek() else {
            let close = stack.last().copied().unwrap_or('}');
            return Err(builtin::expected_symbol(reader, &close.to_string()));
        };
        if StringReader::is_quoted_string_start(c) && !stack.is_empty() {
            reader.read_quoted_string()?;
            continue;
        }
        reader.skip();
        if let Some(close) = closing(c) {
            stack.push(close);
        } else if stack.last() == Some(&c) {
            stack.pop();
            if stack.is_empty() {
                return Ok(());
            }
        }
    }
}

/// Reads one "word" of an opaque argument: up to the next top-level space, keeping
/// bracketed groups and quoted strings intact. Returns the raw source text.
pub fn read_token(reader: &mut StringReader) -> Result<String, CommandSyntaxError> {
    let start = reader.cursor();
    while let Some(c) = reader.peek() {
        if c == ARGUMENT_SEPARATOR {
            break;
        }
        if StringReader::is_quoted_string_start(c) {
            reader.read_quoted_string()?;
        } else if closing(c).is_some() {
            read_balanced(reader)?;
        } else {
            reader.skip();
        }
    }
    let token = reader.string()[start..reader.cursor()].to_owned();
    if token.is_empty() {
        return Err(builtin::parse_exception(reader, "expected a value"));
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert_eq!(normalize_identifier("stone").unwrap(), "minecraft:stone");
        assert_eq!(normalize_identifier("a.b:c/d").unwrap(), "a.b:c/d");
        assert_eq!(normalize_identifier(":x").unwrap(), "minecraft:x");
        assert!(normalize_identifier("a/b:c").is_none());
        assert!(normalize_identifier("a:b:c").is_none());

        let mut r = StringReader::new("Foo");
        assert_eq!(read_identifier(&mut r).unwrap(), "minecraft:");
        let mut r = StringReader::new("a:b:c x");
        let e = read_identifier(&mut r).unwrap_err();
        assert_eq!(e.message(), "Invalid ID");
        assert_eq!(e.cursor(), Some(0));
    }

    #[test]
    fn tokens_keep_groups_together() {
        let mut r = StringReader::new(r#"diamond[custom_name={text:"a ] b"}] 5"#);
        assert_eq!(
            read_token(&mut r).unwrap(),
            r#"diamond[custom_name={text:"a ] b"}]"#
        );
        assert_eq!(r.remaining(), " 5");
        let mut r = StringReader::new("{a:[1, 2}");
        assert_eq!(read_token(&mut r).unwrap_err().message(), "Expected ']'");
    }
}
