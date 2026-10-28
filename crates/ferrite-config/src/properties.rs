//! A lossless-enough model of Java `.properties` files, as used by `server.properties`.
//!
//! Vanilla writes its properties file through `java.util.Properties`, so this module follows
//! the same escaping rules and keeps unknown keys around when a file is rewritten.

/// One key in a typed properties schema, with the comment written above it in generated files.
#[derive(Debug, Clone, Copy)]
pub struct PropertyKey {
    /// The `server.properties` key.
    pub key: &'static str,
    /// Comment emitted above the key in generated files.
    pub comment: &'static str,
}

/// An ordered `server.properties` document. Unknown keys are retained verbatim.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesDocument {
    entries: Vec<(String, String)>,
}

impl PropertiesDocument {
    /// Parse Java `.properties` text.
    ///
    /// Blank lines and lines starting with `#` or `!` are ignored. Backslash line
    /// continuations and Java escapes (`\t`, `\n`, `\r`, `\f`, `\uXXXX`) are decoded.
    /// For duplicate keys the last value wins (like `java.util.Properties`).
    pub fn parse(text: &str) -> Self {
        let mut doc = Self::default();
        let mut logical = String::new();
        let mut continuing = false;

        for raw_line in text.lines() {
            let raw_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            let line = if continuing {
                // Java skips leading whitespace on continuation lines.
                raw_line.trim_start()
            } else {
                let trimmed = raw_line.trim_start();
                if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                    continue;
                }
                trimmed
            };

            logical.push_str(line);
            if has_unescaped_trailing_backslash(&logical) {
                logical.pop();
                continuing = true;
            } else {
                parse_logical_line(&logical, &mut doc);
                logical.clear();
                continuing = false;
            }
        }

        if !logical.is_empty() {
            parse_logical_line(&logical, &mut doc);
        }

        doc
    }

    /// Read a key, returning the decoded value.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value.as_str())
    }

    /// Insert or replace a key. Existing keys keep their position.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if let Some((_, existing)) = self
            .entries
            .iter_mut()
            .find(|(candidate, _)| *candidate == key)
        {
            *existing = value;
        } else {
            self.entries.push((key, value));
        }
    }

    /// Remove a key, returning its value.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let index = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == key)?;
        Some(self.entries.remove(index).1)
    }

    /// Iterate over all key/value pairs in file order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    /// Number of entries, including unknown keys.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the document has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Render the document exactly like vanilla Java properties: keys sorted, no comments.
    pub fn to_plain_string(&self) -> String {
        self.render(&[])
    }

    /// Render the document with a vanilla-style header and the supplied key comments.
    ///
    /// Unknown keys are written too, sorted among the known keys, so a rewrite never drops
    /// third-party or newer-version settings.
    pub fn to_string_with_comments(&self, schema: &[PropertyKey]) -> String {
        self.render(schema)
    }

    fn render(&self, schema: &[PropertyKey]) -> String {
        let mut sorted: Vec<&(String, String)> = self.entries.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        let mut out = String::from("#Minecraft server properties\n");
        for (key, value) in sorted {
            if let Some(comment) = schema.iter().find(|entry| entry.key == key) {
                for line in comment.comment.lines() {
                    out.push_str("# ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            out.push_str(&escape_property(key, true));
            out.push('=');
            out.push_str(&escape_property(value, false));
            out.push('\n');
        }
        out
    }
}

fn parse_logical_line(line: &str, doc: &mut PropertiesDocument) {
    let bytes = line.as_bytes();
    let mut separator = None;
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'=' | b':' => {
                separator = Some((index, index + 1));
                break;
            }
            byte if byte.is_ascii_whitespace() => {
                separator = Some((index, index));
                break;
            }
            _ => {}
        }
    }

    let (key_end, mut value_start) = separator.unwrap_or((line.len(), line.len()));
    if value_start == key_end {
        // Whitespace separator: skip whitespace, then an optional `=` or `:`, then whitespace.
        while value_start < bytes.len() && bytes[value_start].is_ascii_whitespace() {
            value_start += 1;
        }
        if value_start < bytes.len() && (bytes[value_start] == b'=' || bytes[value_start] == b':') {
            value_start += 1;
        }
    }
    while value_start < bytes.len() && bytes[value_start].is_ascii_whitespace() {
        value_start += 1;
    }

    let key = unescape_property(&line[..key_end]);
    if key.is_empty() {
        return;
    }
    let value = unescape_property(&line[value_start..]);
    doc.set(key, value);
}

fn has_unescaped_trailing_backslash(line: &str) -> bool {
    let mut backslashes = 0;
    for byte in line.as_bytes().iter().rev() {
        if *byte == b'\\' {
            backslashes += 1;
        } else {
            break;
        }
    }
    backslashes % 2 == 1
}

fn unescape_property(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            None => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('f') => out.push('\u{000c}'),
            Some('u') => {
                let mut hex = String::new();
                for _ in 0..4 {
                    if let Some(ch) = chars.next() {
                        hex.push(ch);
                    }
                }
                if let Ok(code) = u32::from_str_radix(&hex, 16)
                    && let Some(ch) = char::from_u32(code)
                {
                    out.push(ch);
                    continue;
                }
                out.push_str("\\u");
                out.push_str(&hex);
            }
            Some(other) => out.push(other),
        }
    }
    out
}

fn escape_property(text: &str, is_key: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, ch) in text.chars().enumerate() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{000c}' => out.push_str("\\f"),
            ' ' if index == 0 => out.push_str("\\ "),
            '=' | ':' => {
                out.push('\\');
                out.push(ch);
            }
            '#' | '!' if index == 0 || is_key => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out
}

/// A value that can be read from and written to a `server.properties` entry.
pub trait PropertyValue: Sized {
    /// Parse a decoded property value.
    fn parse_property(raw: &str) -> Result<Self, String>;

    /// Render the value in the form vanilla writes it.
    fn render_property(&self) -> String;
}

impl PropertyValue for String {
    fn parse_property(raw: &str) -> Result<Self, String> {
        Ok(raw.to_string())
    }

    fn render_property(&self) -> String {
        self.clone()
    }
}

impl PropertyValue for bool {
    fn parse_property(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(format!("expected `true` or `false`, found `{other}`")),
        }
    }

    fn render_property(&self) -> String {
        self.to_string()
    }
}

macro_rules! integer_property_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PropertyValue for $ty {
                fn parse_property(raw: &str) -> Result<Self, String> {
                    raw.trim()
                        .parse::<Self>()
                        .map_err(|error| format!("expected an integer, found `{}`: {error}", raw.trim()))
                }

                fn render_property(&self) -> String {
                    self.to_string()
                }
            }
        )*
    };
}

integer_property_value!(i32, i64);

/// Define a typed `server.properties` schema.
///
/// Each field is emitted in the struct, `Default`, the key table, the loader and the writer,
/// so keys, types, defaults and comments cannot drift apart.
#[macro_export]
macro_rules! define_properties {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field:ident : $ty:ty = $default:expr => $key:literal, $comment:literal
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name {
            $(
                $(#[$field_meta])*
                pub $field: $ty,
            )*
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $($field: $default,)* }
            }
        }

        impl $name {
            /// Every key in this schema with its comment, in declaration order.
            pub const KEY_TABLE: &'static [$crate::properties::PropertyKey] = &[
                $(
                    $crate::properties::PropertyKey { key: $key, comment: $comment },
                )*
            ];

            /// Load typed values from a document, filling missing keys with defaults.
            pub fn from_document(
                document: &$crate::properties::PropertiesDocument,
            ) -> Result<Self, $crate::error::ConfigError> {
                Ok(Self {
                    $(
                        $field: match document.get($key) {
                            Some(raw) => <$ty as $crate::properties::PropertyValue>::parse_property(raw)
                                .map_err(|message| $crate::error::ConfigError::Property {
                                    key: $key.to_string(),
                                    message,
                                })?,
                            None => $default,
                        },
                    )*
                })
            }

            /// Write every known key into a document, leaving unknown keys untouched.
            pub fn apply_to(&self, document: &mut $crate::properties::PropertiesDocument) {
                $(
                    document.set(
                        $key,
                        <$ty as $crate::properties::PropertyValue>::render_property(&self.$field),
                    );
                )*
            }

            /// Render the value of a known key, or `None` for an unknown key.
            pub fn get_key(&self, key: &str) -> Option<String> {
                match key {
                    $(
                        $key => Some(<$ty as $crate::properties::PropertyValue>::render_property(
                            &self.$field,
                        )),
                    )*
                    _ => None,
                }
            }

            /// The rendered default for a known key.
            pub fn default_key_value(key: &str) -> Option<String> {
                match key {
                    $(
                        $key => Some(<$ty as $crate::properties::PropertyValue>::render_property(
                            &$default,
                        )),
                    )*
                    _ => None,
                }
            }
        }
    };
}
