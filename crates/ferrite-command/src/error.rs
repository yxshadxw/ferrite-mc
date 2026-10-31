//! Syntax errors (with cursor context) and command execution failures.
//!
//! Every error carries the vanilla translation key and its arguments, so a server can send
//! a translatable component, plus an English rendering matching the 26.3 `en_us` strings.

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use crate::reader::StringReader;

/// Characters of input shown before the cursor in error context (Brigadier's `CONTEXT_AMOUNT`).
pub const CONTEXT_AMOUNT: usize = 10;

/// A parse failure: message, translation key/arguments and (optionally) input + cursor.
///
/// The cursor is a byte offset into the input; [`CommandSyntaxError::position`] converts it
/// to the UTF-16 index Java (and thus the vanilla client) reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSyntaxError {
    key: Cow<'static, str>,
    args: Vec<String>,
    message: String,
    input: Option<Arc<str>>,
    cursor: usize,
}

/// The pieces vanilla's `Commands.performCommand` shows below a failed command:
/// `[...]<before><after, red underlined><--[HERE]>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorContext {
    /// Whether the visible text was truncated on the left (vanilla prints `...`).
    pub ellipsis: bool,
    /// Up to [`CONTEXT_AMOUNT`] characters before the cursor.
    pub before: String,
    /// Everything after the cursor.
    pub after: String,
}

impl CommandSyntaxError {
    /// An error with no input context.
    pub fn new(key: impl Into<Cow<'static, str>>, args: Vec<String>, message: String) -> Self {
        Self {
            key: key.into(),
            args,
            message,
            input: None,
            cursor: 0,
        }
    }

    /// Attaches `reader`'s input and cursor.
    pub fn with_context(mut self, reader: &StringReader) -> Self {
        self.input = Some(reader.string_arc());
        self.cursor = reader.cursor();
        self
    }

    pub fn translation_key(&self) -> &str {
        &self.key
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// English message without context.
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn input(&self) -> Option<&str> {
        self.input.as_deref()
    }

    /// Byte cursor into [`Self::input`], if there is an input.
    pub fn cursor(&self) -> Option<usize> {
        self.input.as_ref().map(|_| self.cursor)
    }

    /// Cursor as a UTF-16 index (what Java's `getCursor()` returns).
    pub fn position(&self) -> Option<usize> {
        let input = self.input.as_deref()?;
        Some(input[..self.cursor.min(input.len())].encode_utf16().count())
    }

    /// Brigadier's `getContext()`: `...` + 10 characters before the cursor + `<--[HERE]`.
    pub fn context(&self) -> Option<String> {
        let ctx = self.error_context()?;
        Some(format!(
            "{}{}<--[HERE]",
            if ctx.ellipsis { "..." } else { "" },
            ctx.before
        ))
    }

    /// The structured context vanilla renders in chat.
    pub fn error_context(&self) -> Option<ErrorContext> {
        let input = self.input.as_deref()?;
        let cursor = self.cursor.min(input.len());
        let head = &input[..cursor];
        let count = head.chars().count();
        let skip = count.saturating_sub(CONTEXT_AMOUNT);
        let start = head.char_indices().nth(skip).map_or(cursor, |(i, _)| i);
        Some(ErrorContext {
            ellipsis: count > CONTEXT_AMOUNT,
            before: input[start..cursor].to_owned(),
            after: input[cursor..].to_owned(),
        })
    }
}

impl fmt::Display for CommandSyntaxError {
    /// `command.context.parse_error`: `%s at position %s: %s`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.context(), self.position()) {
            (Some(context), Some(position)) => {
                write!(f, "{} at position {position}: {context}", self.message)
            }
            _ => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for CommandSyntaxError {}

/// Why running a command failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")]
    Syntax(#[from] CommandSyntaxError),
    /// The command parsed but refused to run (Brigadier's `SimpleCommandExceptionType`).
    #[error("{0}")]
    Failed(String),
}

impl CommandError {
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }
}

/// Formats a double like Java's `Double.toString` (`1.0`, `5.9999968E7`, `1.0E-5`).
pub fn java_double(value: f64) -> String {
    java_float_fmt(value, format!("{value}"), format!("{value:e}"))
}

/// Formats a float like Java's `Float.toString`.
pub fn java_float(value: f32) -> String {
    java_float_fmt(f64::from(value), format!("{value}"), format!("{value:e}"))
}

fn java_float_fmt(value: f64, plain: String, sci: String) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let abs = value.abs();
    if abs == 0.0 || (1e-3..1e7).contains(&abs) {
        if plain.contains('.') {
            plain
        } else {
            plain + ".0"
        }
    } else {
        let (mantissa, exp) = sci.split_once('e').expect("exponent format");
        let mantissa = if mantissa.contains('.') {
            mantissa.to_owned()
        } else {
            format!("{mantissa}.0")
        };
        format!("{mantissa}E{exp}")
    }
}

/// An error with an arbitrary translation key at `reader`'s cursor.
pub fn error_from(
    key: &'static str,
    args: Vec<String>,
    message: String,
    reader: &StringReader,
) -> CommandSyntaxError {
    CommandSyntaxError::new(key, args, message).with_context(reader)
}

fn err(key: &'static str, args: Vec<String>, message: String) -> CommandSyntaxError {
    CommandSyntaxError::new(key, args, message)
}

/// Constructors for the built-in errors (Brigadier's `BuiltInExceptions` with vanilla's
/// 26.3 translations). Each takes the reader whose cursor marks the error.
pub mod builtin {
    use super::*;

    macro_rules! simple {
        ($($fn:ident => $key:literal, $msg:literal;)*) => {$(
            pub fn $fn(reader: &StringReader) -> CommandSyntaxError {
                err($key, Vec::new(), $msg.into()).with_context(reader)
            }
        )*};
    }

    macro_rules! one {
        ($($fn:ident => $key:literal, $fmt:literal;)*) => {$(
            pub fn $fn(reader: &StringReader, value: &str) -> CommandSyntaxError {
                err($key, vec![value.to_owned()], format!($fmt, value)).with_context(reader)
            }
        )*};
    }

    macro_rules! bound {
        ($($fn:ident => $key:literal, $what:literal, $rel:literal;)*) => {$(
            pub fn $fn(reader: &StringReader, found: &str, limit: &str) -> CommandSyntaxError {
                err(
                    $key,
                    vec![limit.to_owned(), found.to_owned()],
                    format!(concat!($what, " must not be ", $rel, " than {}: found {}"), limit, found),
                )
                .with_context(reader)
            }
        )*};
    }

    simple! {
        expected_start_of_quote => "parsing.quote.expected.start", "Expected quote to start a string";
        expected_end_of_quote => "parsing.quote.expected.end", "Unclosed quoted string";
        expected_bool => "parsing.bool.expected", "Expected boolean";
        expected_int => "parsing.int.expected", "Expected integer";
        expected_long => "parsing.long.expected", "Expected long";
        expected_double => "parsing.double.expected", "Expected double";
        expected_float => "parsing.float.expected", "Expected float";
        unknown_command => "command.unknown.command", "Unknown or incomplete command. See below for error";
        unknown_argument => "command.unknown.argument", "Incorrect argument for command";
        expected_argument_separator => "command.expected.separator", "Expected whitespace to end one argument, but found trailing data";
    }

    one! {
        invalid_escape => "parsing.quote.escape", "Invalid escape sequence '\\{}' in quoted string";
        invalid_bool => "parsing.bool.invalid", "Invalid boolean: expected 'true' or 'false' but found '{}'";
        invalid_int => "parsing.int.invalid", "Invalid integer '{}'";
        invalid_long => "parsing.long.invalid", "Invalid long '{}'";
        invalid_double => "parsing.double.invalid", "Invalid double '{}'";
        invalid_float => "parsing.float.invalid", "Invalid float '{}'";
        expected_symbol => "parsing.expected", "Expected '{}'";
        literal_incorrect => "argument.literal.incorrect", "Expected literal {}";
    }

    bound! {
        integer_too_low => "argument.integer.low", "Integer", "less";
        integer_too_high => "argument.integer.big", "Integer", "more";
        long_too_low => "argument.long.low", "Long", "less";
        long_too_high => "argument.long.big", "Long", "more";
        float_too_low => "argument.float.low", "Float", "less";
        float_too_high => "argument.float.big", "Float", "more";
        double_too_low => "argument.double.low", "Double", "less";
        double_too_high => "argument.double.big", "Double", "more";
    }

    /// `command.exception`: wraps an unexpected error from a parser.
    pub fn parse_exception(reader: &StringReader, message: &str) -> CommandSyntaxError {
        err(
            "command.exception",
            vec![message.to_owned()],
            format!("Could not parse command: {message}"),
        )
        .with_context(reader)
    }
}

/// Constructors for Minecraft argument errors (keys from the vanilla language file).
pub mod minecraft {
    use super::*;

    macro_rules! simple {
        ($($fn:ident => $key:literal, $msg:literal;)*) => {$(
            pub fn $fn(reader: &StringReader) -> CommandSyntaxError {
                err($key, Vec::new(), $msg.into()).with_context(reader)
            }
        )*};
    }

    macro_rules! one {
        ($($fn:ident => $key:literal, $fmt:literal;)*) => {$(
            pub fn $fn(reader: &StringReader, value: &str) -> CommandSyntaxError {
                err($key, vec![value.to_owned()], format!($fmt, value)).with_context(reader)
            }
        )*};
    }

    simple! {
        entity_invalid => "argument.entity.invalid", "Invalid name or UUID";
        selector_missing => "argument.entity.selector.missing", "Missing selector type";
        selectors_not_allowed => "argument.entity.selector.not_allowed", "Selector not allowed";
        options_unterminated => "argument.entity.options.unterminated", "Expected end of options";
        entity_too_many => "argument.entity.toomany", "Only one entity is allowed, but the provided selector allows more than one";
        player_too_many => "argument.player.toomany", "Only one player is allowed, but the provided selector allows more than one";
        player_only => "argument.player.entities", "Only players may be affected by this command, but the provided selector includes entities";
        pos_missing_double => "argument.pos.missing.double", "Expected a coordinate";
        pos_missing_int => "argument.pos.missing.int", "Expected a block position";
        pos_mixed => "argument.pos.mixed", "Cannot mix world and local coordinates (everything must either use ^ or not)";
        pos2d_incomplete => "argument.pos2d.incomplete", "Incomplete (expected 2 coordinates)";
        pos3d_incomplete => "argument.pos3d.incomplete", "Incomplete (expected 3 coordinates)";
        id_invalid => "argument.id.invalid", "Invalid ID";
        time_invalid_unit => "argument.time.invalid_unit", "Invalid unit";
        limit_too_small => "argument.entity.options.limit.toosmall", "Limit must be at least 1";
    }

    one! {
        selector_unknown => "argument.entity.selector.unknown", "Unknown selector type '{}'";
        gamemode_invalid => "argument.gamemode.invalid", "Unknown game mode: {}";
        option_valueless => "argument.entity.options.valueless", "Expected value for option '{}'";
        option_unknown => "argument.entity.options.unknown", "Unknown option '{}'";
        anchor_invalid => "argument.anchor.invalid", "Invalid entity anchor position {}";
    }

    pub fn time_too_low(reader: &StringReader, found: i32, min: i32) -> CommandSyntaxError {
        err(
            "argument.time.tick_count_too_low",
            vec![min.to_string(), found.to_string()],
            format!("The tick count must not be less than {min}: found {found}"),
        )
        .with_context(reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_number_formatting() {
        assert_eq!(java_double(1.0), "1.0");
        assert_eq!(java_double(-0.5), "-0.5");
        assert_eq!(java_double(59999968.0), "5.9999968E7");
        assert_eq!(java_double(1e-5), "1.0E-5");
        assert_eq!(java_double(0.0), "0.0");
        assert_eq!(java_float(10000.0), "10000.0");
        assert_eq!(java_float(1e-5), "1.0E-5");
        assert_eq!(java_float(f32::MAX), "3.4028235E38");
    }

    #[test]
    fn context_truncates_to_ten_chars() {
        let mut reader = StringReader::new("gamemode creative extra");
        reader.set_cursor(17);
        let e = builtin::expected_argument_separator(&reader);
        assert_eq!(e.context().unwrap(), "...e creative<--[HERE]");
        assert_eq!(
            e.to_string(),
            "Expected whitespace to end one argument, but found trailing data at position 17: ...e creative<--[HERE]"
        );
        let ctx = e.error_context().unwrap();
        assert!(ctx.ellipsis);
        assert_eq!(ctx.after, " extra");
    }

    #[test]
    fn position_counts_utf16() {
        let mut reader = StringReader::new("say \u{1F600}x");
        reader.set_cursor("say \u{1F600}".len());
        let e = builtin::unknown_argument(&reader);
        assert_eq!(e.position(), Some(6));
        assert_eq!(e.cursor(), Some(8));
    }
}
