//! `StringReader`: the character source every parser reads from.
//!
//! This is a port of Brigadier's `com.mojang.brigadier.StringReader` (MIT), including its
//! character classes and error behaviour, so that command text parses exactly like vanilla's.
//!
//! Cursor positions count Unicode scalar values; Brigadier counts UTF-16 code units. The two
//! agree for every command that is not astral-plane text.

/// Escape character inside quoted strings.
pub const SYNTAX_ESCAPE: char = '\\';
/// Double quote starting (and ending) a quoted string.
pub const SYNTAX_DOUBLE_QUOTE: char = '"';
/// Single quote starting (and ending) a quoted string.
pub const SYNTAX_SINGLE_QUOTE: char = '\'';

use crate::error::{self, CommandSyntaxError};

/// A cursor over a command string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringReader {
    text: Vec<char>,
    cursor: usize,
}

impl Default for StringReader {
    fn default() -> Self {
        Self::new("")
    }
}

impl StringReader {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into().chars().collect(),
            cursor: 0,
        }
    }

    /// The full text being read.
    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// Total length in characters.
    pub fn total_length(&self) -> usize {
        self.text.len()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        debug_assert!(cursor <= self.text.len(), "cursor past the end of the input");
        self.cursor = cursor;
    }

    /// Characters already consumed.
    pub fn read(&self) -> String {
        self.text[..self.cursor].iter().collect()
    }

    /// Characters not yet consumed.
    pub fn remaining(&self) -> String {
        self.reread(self.cursor)
    }

    fn reread(&self, from: usize) -> String {
        self.text[from.min(self.text.len())..].iter().collect()
    }

    pub fn can_read(&self) -> bool {
        self.can_read_length(1)
    }

    pub fn can_read_length(&self, length: usize) -> bool {
        self.cursor + length <= self.text.len()
    }

    /// The next character without consuming it.
    pub fn peek(&self) -> char {
        self.text[self.cursor]
    }

    /// The character at `offset` from the cursor without consuming it.
    pub fn peek_offset(&self, offset: usize) -> char {
        self.text[self.cursor + offset]
    }

    /// Consumes and returns the next character.
    pub fn read_char(&mut self) -> char {
        let c = self.text[self.cursor];
        self.cursor += 1;
        c
    }

    /// Consumes one character without returning it.
    pub fn skip(&mut self) {
        self.cursor += 1;
    }

    /// Consumes all following whitespace (Brigadier uses [`char::is_whitespace`]).
    pub fn skip_whitespace(&mut self) {
        while self.can_read() && self.peek().is_whitespace() {
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
        c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '+'
    }

    /// Reads characters allowed in an unquoted string; may return an empty string.
    pub fn read_unquoted_string(&mut self) -> String {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_in_unquoted_string(self.peek()) {
            self.skip();
        }
        self.slice(start, self.cursor)
    }

    /// Reads a single- or double-quoted string, processing `\\` escapes.
    pub fn read_quoted_string(&mut self) -> Result<String, CommandSyntaxError> {
        if !self.can_read() {
            return Ok(String::new());
        }
        let next = self.peek();
        if !Self::is_quoted_string_start(next) {
            return Err(error::expected_start_of_quote(self));
        }
        self.skip();
        self.read_string_until(next)
    }

    /// Reads until `terminator`, honouring `\\` escapes of the terminator and of `\\` itself.
    pub fn read_string_until(&mut self, terminator: char) -> Result<String, CommandSyntaxError> {
        let mut result = String::new();
        let mut escaped = false;
        while self.can_read() {
            let c = self.read_char();
            if escaped {
                if c == terminator || c == SYNTAX_ESCAPE {
                    result.push(c);
                    escaped = false;
                } else {
                    self.cursor -= 1;
                    return Err(error::invalid_escape(self, c));
                }
            } else if c == SYNTAX_ESCAPE {
                escaped = true;
            } else if c == terminator {
                return Ok(result);
            } else {
                result.push(c);
            }
        }
        Err(error::expected_end_of_quote(self))
    }

    /// Reads a quoted string if the cursor sits on a quote, an unquoted string otherwise.
    pub fn read_string(&mut self) -> Result<String, CommandSyntaxError> {
        if !self.can_read() {
            return Ok(String::new());
        }
        let next = self.peek();
        if Self::is_quoted_string_start(next) {
            self.skip();
            self.read_string_until(next)
        } else {
            Ok(self.read_unquoted_string())
        }
    }

    /// Reads everything up to the end of the input.
    pub fn read_remaining(&mut self) -> String {
        let rest = self.remaining();
        self.cursor = self.text.len();
        rest
    }

    pub fn read_bool(&mut self) -> Result<bool, CommandSyntaxError> {
        let start = self.cursor;
        let value = self.read_string()?;
        if value.is_empty() {
            return Err(error::expected_bool(self));
        }
        if value == "true" {
            return Ok(true);
        }
        if value == "false" {
            return Ok(false);
        }
        self.cursor = start;
        Err(error::invalid_bool(self, &value))
    }

    fn read_number(&mut self, kind: NumberKind) -> Result<String, CommandSyntaxError> {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_number(self.peek()) {
            self.skip();
        }
        let number = self.slice(start, self.cursor);
        if number.is_empty() {
            return Err(match kind {
                NumberKind::Int => error::expected_int(self),
                NumberKind::Long => error::expected_long(self),
                NumberKind::Float => error::expected_float(self),
                NumberKind::Double => error::expected_double(self),
            });
        }
        Ok(number)
    }

    pub fn read_int(&mut self) -> Result<i32, CommandSyntaxError> {
        let start = self.cursor;
        let number = self.read_number(NumberKind::Int)?;
        match number.parse::<i32>() {
            Ok(value) => Ok(value),
            Err(_) => {
                self.cursor = start;
                Err(error::invalid_int(self, &number))
            }
        }
    }

    pub fn read_long(&mut self) -> Result<i64, CommandSyntaxError> {
        let start = self.cursor;
        let number = self.read_number(NumberKind::Long)?;
        match number.parse::<i64>() {
            Ok(value) => Ok(value),
            Err(_) => {
                self.cursor = start;
                Err(error::invalid_long(self, &number))
            }
        }
    }

    pub fn read_double(&mut self) -> Result<f64, CommandSyntaxError> {
        let start = self.cursor;
        let number = self.read_number(NumberKind::Double)?;
        match number.parse::<f64>() {
            Ok(value) => Ok(value),
            Err(_) => {
                self.cursor = start;
                Err(error::invalid_double(self, &number))
            }
        }
    }

    pub fn read_float(&mut self) -> Result<f32, CommandSyntaxError> {
        let start = self.cursor;
        let number = self.read_number(NumberKind::Float)?;
        match number.parse::<f32>() {
            Ok(value) => Ok(value),
            Err(_) => {
                self.cursor = start;
                Err(error::invalid_float(self, &number))
            }
        }
    }

    /// Consumes one expected character or fails with `Expected 'c'`.
    pub fn expect(&mut self, c: char) -> Result<(), CommandSyntaxError> {
        if !self.can_read() || self.peek() != c {
            return Err(error::at(format!("Expected '{c}'"), self));
        }
        self.skip();
        Ok(())
    }

    /// Reads characters while `pred` holds.
    pub fn read_while(&mut self, pred: impl Fn(char) -> bool) -> String {
        let start = self.cursor;
        while self.can_read() && pred(self.peek()) {
            self.skip();
        }
        self.slice(start, self.cursor)
    }

    /// The text between two cursors.
    pub fn slice(&self, start: usize, end: usize) -> String {
        self.text[start..end].iter().collect()
    }

    /// True when the cursor sits at the end of the input or just before a space.
    pub fn at_word_end(&self) -> bool {
        !self.can_read() || self.peek() == ' '
    }
}

#[derive(Debug, Clone, Copy)]
enum NumberKind {
    Int,
    Long,
    Float,
    Double,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_strings_and_escapes() {
        let mut reader = StringReader::new(r#""a \"quoted\" \\ value""#);
        assert_eq!(reader.read_quoted_string().unwrap(), r#"a "quoted" \ value"#);
        assert!(!reader.can_read());

        let mut reader = StringReader::new("'single'");
        assert_eq!(reader.read_quoted_string().unwrap(), "single");

        let mut reader = StringReader::new(r#""bad \n escape""#);
        let error = reader.read_quoted_string().unwrap_err();
        assert_eq!(error.message(), "Invalid escape sequence 'n' in quoted string");
        // Brigadier backs the cursor up to the offending character.
        assert_eq!(error.cursor(), Some(6));

        let mut reader = StringReader::new(r#""unclosed"#);
        assert_eq!(reader.read_quoted_string().unwrap_err().message(), "Unclosed quoted string");

        let mut reader = StringReader::new("plain");
        assert_eq!(reader.read_quoted_string().unwrap_err().message(), "Expected quote to start a string");
    }

    #[test]
    fn numbers_follow_brigadier_character_classes() {
        let mut reader = StringReader::new("1234 rest");
        assert_eq!(reader.read_int().unwrap(), 1234);

        let mut reader = StringReader::new("-12");
        assert_eq!(reader.read_int().unwrap(), -12);

        // A number stops at the first character outside [0-9.-], so `1e5` reads as `1`.
        let mut reader = StringReader::new("1e5");
        assert_eq!(reader.read_int().unwrap(), 1);
        assert_eq!(reader.peek(), 'e');

        let mut reader = StringReader::new("1.2.3");
        assert_eq!(reader.read_int().unwrap_err().message(), "Invalid integer '1.2.3'");
        assert_eq!(reader.cursor(), 0, "cursor resets on failure");

        let mut reader = StringReader::new("x");
        assert_eq!(reader.read_int().unwrap_err().message(), "Expected integer");

        let mut reader = StringReader::new("1.5");
        assert_eq!(reader.read_double().unwrap(), 1.5);

        let mut reader = StringReader::new("notabool");
        assert_eq!(
            reader.read_bool().unwrap_err().message(),
            "Invalid bool, expected true or false but found 'notabool'"
        );
        assert_eq!(reader.cursor(), 0);
    }

    #[test]
    fn unquoted_strings_exclude_colons() {
        // Brigadier's unquoted set is [0-9A-Za-z_.+-]; `:` and `/` need their own parsers.
        let mut reader = StringReader::new("minecraft:stone");
        assert_eq!(reader.read_unquoted_string(), "minecraft");
        assert_eq!(reader.peek(), ':');
    }

    #[test]
    fn slices_and_ranges() {
        let mut reader = StringReader::new("time set day");
        let start = reader.cursor();
        reader.read_while(|c| c != ' ');
        assert_eq!(reader.slice(start, reader.cursor()), "time");
        assert_eq!(reader.remaining(), " set day");
        reader.skip_whitespace();
        assert_eq!(reader.remaining(), "set day");
    }
}
