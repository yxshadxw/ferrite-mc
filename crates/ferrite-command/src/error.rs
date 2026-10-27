//! Syntax errors carrying a cursor and the command text, plus execution failures.
//!
//! The messages reproduce Brigadier's built-in exception texts (see
//! `com.mojang.brigadier.exceptions.BuiltInExceptions`) and the rendering of
//! [`CommandSyntaxError`] mirrors `CommandSyntaxException.getMessage()`, including the
//! `<--[HERE]` cursor marker the vanilla client shows in chat.

use std::fmt;

/// Characters of input shown before the error cursor.
pub const CONTEXT_AMOUNT: usize = 10;

/// A parse failure: a message, the command text it happened in, and a cursor in characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSyntaxError {
    message: String,
    input: Option<String>,
    cursor: usize,
}

impl CommandSyntaxError {
    /// Error without an input context; used for failures that are not tied to a position.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            input: None,
            cursor: 0,
        }
    }

    /// Error at `reader`'s cursor, in `reader`'s text.
    pub fn at(message: impl Into<String>, reader: &crate::reader::StringReader) -> Self {
        Self {
            message: message.into(),
            input: Some(reader.text()),
            cursor: reader.cursor(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// Cursor (in characters) where parsing failed, if it is known.
    pub fn cursor(&self) -> Option<usize> {
        self.input.as_ref().map(|_| self.cursor)
    }

    /// The command text the error was raised in, if any.
    pub fn input(&self) -> Option<&str> {
        self.input.as_deref()
    }

    /// Brigadier's `getContext()`: the text before the cursor plus a `<--[HERE]` marker.
    pub fn context(&self) -> Option<String> {
        let input = self.input.as_ref()?;
        let chars: Vec<char> = input.chars().collect();
        let cursor = self.cursor.min(chars.len());
        let mut out = String::new();
        if cursor > CONTEXT_AMOUNT {
            out.push_str("...");
        }
        let start = cursor.saturating_sub(CONTEXT_AMOUNT);
        out.extend(&chars[start..cursor]);
        out.push_str("<--[HERE]");
        Some(out)
    }
}

impl fmt::Display for CommandSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        if let Some(context) = self.context() {
            write!(f, " at position {}: {context}", self.cursor)?;
        }
        Ok(())
    }
}

impl std::error::Error for CommandSyntaxError {}

/// Why executing a parsed command failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// The command did not parse (bad argument, unknown node, trailing data, ...).
    #[error("{0}")]
    Syntax(CommandSyntaxError),
    /// The command parsed but refused to run.
    #[error("{0}")]
    Failed(String),
}

impl CommandError {
    /// A command that parsed but failed at runtime, like Brigadier's simple exception types.
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }
}

impl From<CommandSyntaxError> for CommandError {
    fn from(value: CommandSyntaxError) -> Self {
        Self::Syntax(value)
    }
}

// Message constructors, named after Brigadier's built-in exception types. The first group
// matches Brigadier's texts verbatim; the last is Minecraft's user-facing wording for the
// generic dispatcher failure (Brigadier's "Unknown command") that players see in chat.

/// `readerExpectedStartOfQuote`.
pub fn expected_start_of_quote(reader: &crate::reader::StringReader) -> CommandSyntaxError {
    CommandSyntaxError::at("Expected quote to start a string", reader)
}

/// `readerExpectedEndOfQuote`.
pub fn expected_end_of_quote(reader: &crate::reader::StringReader) -> CommandSyntaxError {
    CommandSyntaxError::at("Unclosed quoted string", reader)
}

/// `readerInvalidEscape`.
pub fn invalid_escape(reader: &crate::reader::StringReader, c: char) -> CommandSyntaxError {
    CommandSyntaxError::at(format!("Invalid escape sequence '{c}' in quoted string"), reader)
}

/// `readerExpectedBool`.
pub fn expected_bool(reader: &crate::reader::StringReader) -> CommandSyntaxError {
    CommandSyntaxError::at("Expected bool", reader)
}

/// `readerInvalidBool`.
pub fn invalid_bool(reader: &crate::reader::StringReader, value: &str) -> CommandSyntaxError {
    CommandSyntaxError::at(
        format!("Invalid bool, expected true or false but found '{value}'"),
        reader,
    )
}

/// `readerExpectedInt`.
pub fn expected_int(reader: &crate::reader::StringReader) -> CommandSyntaxError {
    CommandSyntaxError::at("Expected integer", reader)
}

/// `readerInvalidInt`.
pub fn invalid_int(reader: &crate::reader::StringReader, value: &str) -> CommandSyntaxError {
    CommandSyntaxError::at(format!("Invalid integer '{value}'"), reader)
}

/// `readerExpectedLong`.
pub fn expected_long(reader: &crate::reader::StringReader) -> CommandSyntaxError {
    CommandSyntaxError::at("Expected long", reader)
}

/// `readerInvalidLong`.
pub fn invalid_long(reader: &crate::reader::StringReader, value: &str) -> CommandSyntaxError {
    CommandSyntaxError::at(format!("Invalid long '{value}'"), reader)
}

/// `readerExpectedFloat`.
pub fn expected_float(reader: &crate::reader::StringReader) -> CommandSyntaxError {
    CommandSyntaxError::at("Expected float", reader)
}

/// `readerInvalidFloat`.
pub fn invalid_float(reader: &crate::reader::StringReader, value: &str) -> CommandSyntaxError {
    CommandSyntaxError::at(format!("Invalid float '{value}'"), reader)
}

/// `readerExpectedDouble`.
pub fn expected_double(reader: &crate::reader::StringReader) -> CommandSyntaxError {
    CommandSyntaxError::at("Expected double", reader)
}

/// `readerInvalidDouble`.
pub fn invalid_double(reader: &crate::reader::StringReader, value: &str) -> CommandSyntaxError {
    CommandSyntaxError::at(format!("Invalid double '{value}'"), reader)
}

/// `integerTooLow` / `integerTooHigh` …: `{kind} must not be less than {min}, found {found}`.
pub fn number_too_low(
    kind: &str,
    reader: &crate::reader::StringReader,
    found: String,
    min: String,
) -> CommandSyntaxError {
    CommandSyntaxError::at(
        format!("{kind} must not be less than {min}, found {found}"),
        reader,
    )
}

/// `{kind} must not be more than {max}, found {found}`.
pub fn number_too_high(
    kind: &str,
    reader: &crate::reader::StringReader,
    found: String,
    max: String,
) -> CommandSyntaxError {
    CommandSyntaxError::at(
        format!("{kind} must not be more than {max}, found {found}"),
        reader,
    )
}

/// `literalIncorrect`.
pub fn literal_incorrect(reader: &crate::reader::StringReader, name: &str) -> CommandSyntaxError {
    CommandSyntaxError::at(format!("Expected literal {name}"), reader)
}

/// `dispatcherExpectedArgumentSeparator`.
pub fn expected_argument_separator(
    reader: &crate::reader::StringReader,
) -> CommandSyntaxError {
    CommandSyntaxError::at(
        "Expected whitespace to end one argument, but found trailing data",
        reader,
    )
}

/// `dispatcherParseException`.
pub fn parse_exception(
    reader: &crate::reader::StringReader,
    message: &str,
) -> CommandSyntaxError {
    CommandSyntaxError::at(format!("Could not parse command: {message}"), reader)
}

/// `dispatcherUnknownCommand`.
pub fn unknown_command(reader: &crate::reader::StringReader) -> CommandSyntaxError {
    CommandSyntaxError::at("Unknown command", reader)
}

/// `dispatcherUnknownArgument`.
pub fn unknown_argument(reader: &crate::reader::StringReader) -> CommandSyntaxError {
    CommandSyntaxError::at("Incorrect argument for command", reader)
}

/// What the vanilla client prints when no child of the root matched at all.
pub fn unknown_or_incomplete(cursor: usize, input: &str) -> CommandSyntaxError {
    CommandSyntaxError {
        message: "Unknown or incomplete command, see below for error".to_owned(),
        input: Some(input.to_owned()),
        cursor,
    }
}

/// Formats an `f64` the way Java's `Double.toString` does for the values that appear in
/// parser bounds: `1.0`, `-59999968.0`, `1.0E-5`.
pub fn java_double(value: f64) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".to_owned()
        } else if value > 0.0 {
            "Infinity".to_owned()
        } else {
            "-Infinity".to_owned()
        };
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-3..1e7).contains(&magnitude) {
        let formatted = format!("{value:E}");
        let (mantissa, exponent) = formatted.split_once('E').unwrap_or((formatted.as_str(), "0"));
        let mantissa = if mantissa.contains('.') {
            mantissa.to_owned()
        } else {
            format!("{mantissa}.0")
        };
        return format!("{mantissa}E{exponent}");
    }
    let s = format!("{value}");
    if s.contains('.') { s } else { format!("{s}.0") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::StringReader;

    #[test]
    fn error_renders_brigadier_context() {
        let mut reader = StringReader::new("time set -5");
        reader.set_cursor("time set".len());
        let error = invalid_int(&reader, "-5");
        assert_eq!(
            error.to_string(),
            "Invalid integer '-5' at position 8: time set <--[HERE]"
        );
        assert_eq!(error.cursor(), Some(8));
        assert_eq!(error.context().as_deref(), Some("time set<--[HERE]"));
    }

    #[test]
    fn long_context_is_elided() {
        let text = "averylongcommandnamethatkeepsgoing";
        let mut reader = StringReader::new(text);
        reader.set_cursor(text.len());
        let error = CommandSyntaxError::at("boom", &reader);
        let rendered = error.to_string();
        assert!(rendered.ends_with("...thatkeepsgoing<--[HERE]"), "{rendered}");
    }

    #[test]
    fn doubles_print_like_java() {
        assert_eq!(java_double(1.0), "1.0");
        assert_eq!(java_double(-59999968.0), "-59999968.0");
        assert_eq!(java_double(1e-5), "1.0E-5");
        assert_eq!(java_double(0.5), "0.5");
    }
}
