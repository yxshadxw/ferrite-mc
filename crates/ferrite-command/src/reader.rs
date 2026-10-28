//! `StringReader`: Brigadier's character cursor, with identical parsing rules.
//!
//! The cursor is a byte offset into the UTF-8 input and always sits on a char boundary.
//! Brigadier counts UTF-16 units; the two agree for ASCII, and errors convert on output.

use std::sync::Arc;

use crate::error::{CommandSyntaxError, builtin};

pub const SYNTAX_ESCAPE: char = '\\';
pub const SYNTAX_DOUBLE_QUOTE: char = '"';
pub const SYNTAX_SINGLE_QUOTE: char = '\'';
/// Separator between command arguments.
pub const ARGUMENT_SEPARATOR: char = ' ';

/// A cursor over (cheaply clonable) command text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringReader {
    string: Arc<str>,
    cursor: usize,
}

impl StringReader {
    pub fn new(string: impl Into<Arc<str>>) -> Self {
        Self {
            string: string.into(),
            cursor: 0,
        }
    }

    pub fn string(&self) -> &str {
        &self.string
    }

    pub(crate) fn string_arc(&self) -> Arc<str> {
        self.string.clone()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Moves the cursor. Panics if `cursor` is not a char boundary of the input.
    pub fn set_cursor(&mut self, cursor: usize) {
        assert!(
            self.string.is_char_boundary(cursor),
            "cursor {cursor} is not a char boundary"
        );
        self.cursor = cursor;
    }

    pub fn total_length(&self) -> usize {
        self.string.len()
    }

    pub fn remaining_length(&self) -> usize {
        self.string.len() - self.cursor
    }

    /// Text before the cursor.
    pub fn read(&self) -> &str {
        &self.string[..self.cursor]
    }

    /// Text from the cursor on.
    pub fn remaining(&self) -> &str {
        &self.string[self.cursor..]
    }

    pub fn can_read(&self) -> bool {
        self.cursor < self.string.len()
    }

    /// Whether at least `chars` more characters can be read.
    pub fn can_read_chars(&self, chars: usize) -> bool {
        self.remaining()
            .chars()
            .nth(chars.saturating_sub(1))
            .is_some()
            || chars == 0
    }

    pub fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    /// The character `offset` characters after the cursor.
    pub fn peek_at(&self, offset: usize) -> Option<char> {
        self.remaining().chars().nth(offset)
    }

    pub(crate) fn peek_is(&self, c: char) -> bool {
        self.peek() == Some(c)
    }

    /// Consumes one character. Panics at end of input.
    pub fn read_char(&mut self) -> char {
        let c = self.peek().expect("read past end of input");
        self.cursor += c.len_utf8();
        c
    }

    /// Consumes one character if any is left.
    pub fn skip(&mut self) {
        if let Some(c) = self.peek() {
            self.cursor += c.len_utf8();
        }
    }

    /// Skips Java-`Character.isWhitespace` characters.
    pub fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(is_java_whitespace) {
            self.skip();
        }
    }

    pub fn is_allowed_number(c: char) -> bool {
        c.is_ascii_digit() || c == '.' || c == '-'
    }

    pub fn is_quoted_string_start(c: char) -> bool {
        c == SYNTAX_DOUBLE_QUOTE || c == SYNTAX_SINGLE_QUOTE
    }

    pub fn is_allowed_in_unquoted_string(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+')
    }

    fn read_while(&mut self, pred: impl Fn(char) -> bool) -> &str {
        let start = self.cursor;
        while self.peek().is_some_and(&pred) {
            self.skip();
        }
        &self.string[start..self.cursor]
    }

    fn read_number<T: std::str::FromStr>(
        &mut self,
        expected: fn(&StringReader) -> CommandSyntaxError,
        invalid: fn(&StringReader, &str) -> CommandSyntaxError,
    ) -> Result<T, CommandSyntaxError> {
        let start = self.cursor;
        let number = self.read_while(Self::is_allowed_number).to_owned();
        if number.is_empty() {
            return Err(expected(self));
        }
        number.parse().map_err(|_| {
            self.cursor = start;
            invalid(self, &number)
        })
    }

    pub fn read_int(&mut self) -> Result<i32, CommandSyntaxError> {
        self.read_number(builtin::expected_int, builtin::invalid_int)
    }

    pub fn read_long(&mut self) -> Result<i64, CommandSyntaxError> {
        self.read_number(builtin::expected_long, builtin::invalid_long)
    }

    pub fn read_double(&mut self) -> Result<f64, CommandSyntaxError> {
        self.read_number(builtin::expected_double, builtin::invalid_double)
    }

    pub fn read_float(&mut self) -> Result<f32, CommandSyntaxError> {
        self.read_number(builtin::expected_float, builtin::invalid_float)
    }

    /// Reads `[0-9A-Za-z_\-.+]*`; may be empty.
    pub fn read_unquoted_string(&mut self) -> &str {
        self.read_while(Self::is_allowed_in_unquoted_string)
    }

    /// Reads a `"..."` or `'...'` string. Empty input yields an empty string.
    pub fn read_quoted_string(&mut self) -> Result<String, CommandSyntaxError> {
        let Some(next) = self.peek() else {
            return Ok(String::new());
        };
        if !Self::is_quoted_string_start(next) {
            return Err(builtin::expected_start_of_quote(self));
        }
        self.skip();
        self.read_string_until(next)
    }

    /// Reads up to (and consumes) `terminator`; `\` escapes only itself and the terminator.
    pub fn read_string_until(&mut self, terminator: char) -> Result<String, CommandSyntaxError> {
        let mut result = String::new();
        let mut escaped = false;
        while let Some(c) = self.peek() {
            self.skip();
            if escaped {
                if c == terminator || c == SYNTAX_ESCAPE {
                    result.push(c);
                    escaped = false;
                } else {
                    self.cursor -= c.len_utf8();
                    return Err(builtin::invalid_escape(self, &c.to_string()));
                }
            } else if c == SYNTAX_ESCAPE {
                escaped = true;
            } else if c == terminator {
                return Ok(result);
            } else {
                result.push(c);
            }
        }
        Err(builtin::expected_end_of_quote(self))
    }

    /// Reads a quoted string if one starts here, else an unquoted string.
    pub fn read_string(&mut self) -> Result<String, CommandSyntaxError> {
        match self.peek() {
            None => Ok(String::new()),
            Some(c) if Self::is_quoted_string_start(c) => {
                self.skip();
                self.read_string_until(c)
            }
            Some(_) => Ok(self.read_unquoted_string().to_owned()),
        }
    }

    pub fn read_boolean(&mut self) -> Result<bool, CommandSyntaxError> {
        let start = self.cursor;
        let value = self.read_string()?;
        match value.as_str() {
            "" => Err(builtin::expected_bool(self)),
            "true" => Ok(true),
            "false" => Ok(false),
            _ => {
                self.cursor = start;
                Err(builtin::invalid_bool(self, &value))
            }
        }
    }

    /// Consumes `c` or fails with `parsing.expected`.
    pub fn expect(&mut self, c: char) -> Result<(), CommandSyntaxError> {
        if !self.peek_is(c) {
            return Err(builtin::expected_symbol(self, &c.to_string()));
        }
        self.skip();
        Ok(())
    }

    /// Reads everything up to the next argument separator (any characters).
    pub(crate) fn read_until_separator(&mut self) -> &str {
        self.read_while(|c| c != ARGUMENT_SEPARATOR)
    }

    /// Consumes the rest of the input.
    pub(crate) fn read_rest(&mut self) -> &str {
        let start = self.cursor;
        self.cursor = self.string.len();
        &self.string[start..]
    }
}

/// Java's `Character.isWhitespace`: Unicode whitespace except no-break spaces, plus the
/// ASCII separators U+001C..U+001F.
pub fn is_java_whitespace(c: char) -> bool {
    matches!(c, '\u{1C}'..='\u{1F}')
        || (c.is_whitespace() && !matches!(c, '\u{A0}' | '\u{2007}' | '\u{202F}' | '\u{85}'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_strings_and_escapes() {
        let mut r = StringReader::new(r#""hello \"world\" \\ end" rest"#);
        assert_eq!(r.read_quoted_string().unwrap(), r#"hello "world" \ end"#);
        assert_eq!(r.remaining(), " rest");

        let mut r = StringReader::new(r#"'it\'s "fine"'"#);
        assert_eq!(r.read_string().unwrap(), r#"it's "fine""#);
        assert!(!r.can_read());
    }

    #[test]
    fn invalid_escape_points_at_escaped_char() {
        let mut r = StringReader::new(r#""bad \x""#);
        let e = r.read_quoted_string().unwrap_err();
        assert_eq!(e.translation_key(), "parsing.quote.escape");
        assert_eq!(
            e.message(),
            r"Invalid escape sequence '\x' in quoted string"
        );
        assert_eq!(e.cursor(), Some(6));
    }

    #[test]
    fn unclosed_and_unstarted_quotes() {
        let mut r = StringReader::new("\"open");
        assert_eq!(
            r.read_quoted_string().unwrap_err().message(),
            "Unclosed quoted string"
        );
        let mut r = StringReader::new("plain");
        let e = r.read_quoted_string().unwrap_err();
        assert_eq!(e.message(), "Expected quote to start a string");
        assert_eq!(e.cursor(), Some(0));
    }

    #[test]
    fn numbers() {
        let mut r = StringReader::new("-12 3.5 .5 1-2 x");
        assert_eq!(r.read_int().unwrap(), -12);
        r.skip();
        assert_eq!(r.read_double().unwrap(), 3.5);
        r.skip();
        assert_eq!(r.read_float().unwrap(), 0.5);
        r.skip();
        let e = r.read_int().unwrap_err();
        assert_eq!(e.message(), "Invalid integer '1-2'");
        assert_eq!(e.cursor(), Some(11), "cursor reset to number start");
        r.set_cursor(15);
        assert_eq!(r.read_long().unwrap_err().message(), "Expected long");
        assert_eq!(
            StringReader::new("99999999999")
                .read_int()
                .unwrap_err()
                .message(),
            "Invalid integer '99999999999'"
        );
    }

    #[test]
    fn booleans() {
        assert!(StringReader::new("true").read_boolean().unwrap());
        assert!(!StringReader::new("\"false\"").read_boolean().unwrap());
        let e = StringReader::new("yes").read_boolean().unwrap_err();
        assert_eq!(
            e.message(),
            "Invalid boolean: expected 'true' or 'false' but found 'yes'"
        );
        assert_eq!(e.cursor(), Some(0));
        assert_eq!(
            StringReader::new("").read_boolean().unwrap_err().message(),
            "Expected boolean"
        );
    }

    #[test]
    fn unquoted_and_expect() {
        let mut r = StringReader::new("abc_+.-9:rest");
        assert_eq!(r.read_unquoted_string(), "abc_+.-9");
        assert!(r.expect(':').is_ok());
        assert_eq!(r.expect(':').unwrap_err().message(), "Expected ':'");
    }

    #[test]
    fn multibyte_cursor() {
        let mut r = StringReader::new("é ü");
        assert!(r.can_read_chars(3));
        assert!(!r.can_read_chars(4));
        assert_eq!(r.read_char(), 'é');
        r.skip_whitespace();
        assert_eq!(r.peek(), Some('ü'));
    }
}
