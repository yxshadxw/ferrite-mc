//! Legacy `§`/`&` formatting codes.
//!
//! Parsing produces either a single styled text component or a root empty
//! component whose children are the styled runs. Serialization flattens the
//! tree and emits `§r` plus a fresh style prefix whenever the resolved style
//! changes. Styles that legacy codes cannot express (font, insertion, events,
//! shadow color) are dropped.

use crate::style::{NamedColor, Style, TextColor};
use crate::{Component, Content, TextRun};

/// Parses legacy formatting codes. Both `§` and `&` are accepted as prefixes.
pub(crate) fn parse(input: &str) -> Component {
    let chars: Vec<char> = input.chars().collect();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut style = Style::default();
    let mut text = String::new();
    let mut index = 0;

    while index < chars.len() {
        if is_prefix(chars[index]) && index + 1 < chars.len() {
            let code = chars[index + 1];

            if (code == 'x' || code == 'X')
                && let Some((rgb, next)) = parse_hex(&chars, index)
            {
                push_run(&mut runs, &mut text, &style);
                style = Style::default().color(TextColor::rgb(rgb));
                index = next;
                continue;
            }

            if let Some(color) = NamedColor::from_legacy_code(code) {
                push_run(&mut runs, &mut text, &style);
                style = Style::default().color(color);
                index += 2;
                continue;
            }

            if set_format(&mut style, code) {
                index += 2;
                continue;
            }

            if code == 'r' || code == 'R' {
                push_run(&mut runs, &mut text, &style);
                style = Style::default();
                index += 2;
                continue;
            }
        }

        text.push(chars[index]);
        index += 1;
    }
    push_run(&mut runs, &mut text, &style);

    if runs.len() == 1 {
        let run = runs.pop().expect("length checked");
        Component {
            content: Content::Text(run.text),
            style: run.style,
            children: Vec::new(),
        }
    } else {
        Component {
            content: Content::Text(String::new()),
            style: Style::default(),
            children: runs
                .into_iter()
                .map(|run| Component {
                    content: Content::Text(run.text),
                    style: run.style,
                    children: Vec::new(),
                })
                .collect(),
        }
    }
}

/// Serializes a component with legacy formatting codes.
pub(crate) fn to_legacy(component: &Component, prefix: char) -> String {
    let mut out = String::new();
    let mut previous: Option<LegacyFormat> = None;

    for run in component.flatten() {
        if run.text.is_empty() {
            continue;
        }
        let format = LegacyFormat::from_style(&run.style);
        if let Some(previous) = previous {
            if previous != format {
                if !previous.is_default() {
                    out.push(prefix);
                    out.push('r');
                }
                format.write_prefix(&mut out, prefix);
            }
        } else {
            format.write_prefix(&mut out, prefix);
        }
        out.push_str(&run.text);
        previous = Some(format);
    }

    out
}

fn push_run(runs: &mut Vec<TextRun>, text: &mut String, style: &Style) {
    if text.is_empty() {
        return;
    }
    runs.push(TextRun {
        text: std::mem::take(text),
        style: style.clone(),
    });
}

fn is_prefix(c: char) -> bool {
    c == '§' || c == '&'
}

/// Parses `§x§R§R§G§G§B§B` starting at `start`.
fn parse_hex(chars: &[char], start: usize) -> Option<(u32, usize)> {
    let mut rgb = 0u32;
    for digit_index in 0..6 {
        let prefix_index = start + 2 + digit_index * 2;
        let digit_char_index = prefix_index + 1;
        if digit_char_index >= chars.len() || !is_prefix(chars[prefix_index]) {
            return None;
        }
        let digit = chars[digit_char_index].to_digit(16)?;
        rgb = (rgb << 4) | digit;
    }
    Some((rgb, start + 14))
}

/// Applies a formatting code. Returns false when `code` is not a format code.
fn set_format(style: &mut Style, code: char) -> bool {
    match code.to_ascii_lowercase() {
        'k' => style.obfuscated = Some(true),
        'l' => style.bold = Some(true),
        'm' => style.strikethrough = Some(true),
        'n' => style.underlined = Some(true),
        'o' => style.italic = Some(true),
        _ => return false,
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LegacyFormat {
    color: Option<TextColor>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
    obfuscated: bool,
}

impl LegacyFormat {
    fn from_style(style: &Style) -> Self {
        Self {
            color: style.color,
            bold: style.bold.unwrap_or(false),
            italic: style.italic.unwrap_or(false),
            underlined: style.underlined.unwrap_or(false),
            strikethrough: style.strikethrough.unwrap_or(false),
            obfuscated: style.obfuscated.unwrap_or(false),
        }
    }

    fn is_default(self) -> bool {
        self.color.is_none()
            && !self.bold
            && !self.italic
            && !self.underlined
            && !self.strikethrough
            && !self.obfuscated
    }

    fn write_prefix(self, out: &mut String, prefix: char) {
        if let Some(color) = self.color {
            match color {
                TextColor::Named(color) => {
                    out.push(prefix);
                    out.push(color.legacy_code());
                }
                TextColor::Rgb(rgb) => {
                    out.push(prefix);
                    out.push('x');
                    for digit in format!("{:06X}", rgb & 0x00FF_FFFF).chars() {
                        out.push(prefix);
                        out.push(digit);
                    }
                }
            }
        }
        if self.bold {
            out.push(prefix);
            out.push('l');
        }
        if self.italic {
            out.push(prefix);
            out.push('o');
        }
        if self.underlined {
            out.push(prefix);
            out.push('n');
        }
        if self.strikethrough {
            out.push(prefix);
            out.push('m');
        }
        if self.obfuscated {
            out.push(prefix);
            out.push('k');
        }
    }
}

impl Component {
    /// Parses legacy formatting codes (`§` or `&`) into a component tree.
    pub fn from_legacy(input: &str) -> Self {
        parse(input)
    }

    /// Serializes this component with `§` formatting codes.
    pub fn to_legacy(&self) -> String {
        to_legacy(self, '§')
    }

    /// Serializes this component with a custom code prefix, such as `&`.
    pub fn to_legacy_with(&self, prefix: char) -> String {
        to_legacy(self, prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClickEvent, NamedColor};

    #[test]
    fn parses_named_colors_and_formats() {
        assert_eq!(
            Component::from_legacy("§cHello"),
            Component::text("Hello").color(NamedColor::Red)
        );
        assert_eq!(
            Component::from_legacy("§c§lHello"),
            Component::text("Hello").color(NamedColor::Red).bold(true)
        );
        assert_eq!(
            Component::from_legacy("&aGreen"),
            Component::text("Green").color(NamedColor::Green)
        );
    }

    #[test]
    fn parses_hex_colors() {
        let component = Component::from_legacy("§x§1§2§a§b§3§4§lHi");
        assert_eq!(
            component,
            Component::text("Hi")
                .color(TextColor::rgb(0x12_AB_34))
                .bold(true)
        );
    }

    #[test]
    fn hex_serializes_and_round_trips() {
        let component = Component::text("Hi")
            .color(TextColor::rgb(0xFF_55_55))
            .bold(true);
        assert_eq!(component.to_legacy(), "§x§F§F§5§5§5§5§lHi");
        assert_eq!(Component::from_legacy(&component.to_legacy()), component);

        let ampersand = Component::from_legacy("&x&1&2&A&B&3&4Hex");
        assert_eq!(
            ampersand,
            Component::text("Hex").color(TextColor::rgb(0x12_AB_34))
        );
    }

    #[test]
    fn resets_split_runs() {
        let component = Component::from_legacy("§cRed§rGray");
        let runs = component.flatten();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "Red");
        assert_eq!(runs[0].style.color, Some(TextColor::Named(NamedColor::Red)));
        assert_eq!(runs[1].text, "Gray");
        assert_eq!(runs[1].style, Style::default());
    }

    #[test]
    fn complex_styles_round_trip_by_flattening() {
        let component = Component::text("a")
            .color(NamedColor::Red)
            .bold(true)
            .child(
                Component::text("b")
                    .color(NamedColor::Blue)
                    .bold(false)
                    .italic(true),
            )
            .child(Component::text("c"));

        let legacy = component.to_legacy();
        let parsed = Component::from_legacy(&legacy);
        let parsed_runs = parsed.flatten();
        let original_runs = component.flatten();
        assert_eq!(parsed_runs.len(), original_runs.len(), "{legacy}");
        for (parsed, original) in parsed_runs.iter().zip(&original_runs) {
            assert_eq!(parsed.text, original.text, "{legacy}");
            assert_eq!(parsed.style.color, original.style.color, "{legacy}");
            // Legacy codes cannot express an explicit "off"; both forms
            // render identically for a standalone flattened run.
            assert_eq!(
                parsed.style.bold.unwrap_or(false),
                original.style.bold.unwrap_or(false),
                "{legacy}"
            );
            assert_eq!(
                parsed.style.italic.unwrap_or(false),
                original.style.italic.unwrap_or(false),
                "{legacy}"
            );
            assert_eq!(
                parsed.style.underlined.unwrap_or(false),
                original.style.underlined.unwrap_or(false),
                "{legacy}"
            );
            assert_eq!(
                parsed.style.strikethrough.unwrap_or(false),
                original.style.strikethrough.unwrap_or(false),
                "{legacy}"
            );
            assert_eq!(
                parsed.style.obfuscated.unwrap_or(false),
                original.style.obfuscated.unwrap_or(false),
                "{legacy}"
            );
        }
        assert_eq!(parsed.to_plain_text(), "abc");
    }

    #[test]
    fn serializes_with_custom_prefix() {
        let component = Component::text("hi")
            .color(NamedColor::Gold)
            .underlined(true);
        assert_eq!(component.to_legacy_with('&'), "&6&nhi");
        assert_eq!(Component::from_legacy("&6&nhi"), component);
    }

    #[test]
    fn plain_text_drops_codes_and_extended_style() {
        let component = Component::from_legacy("§cHi §lThere")
            .click_event(ClickEvent::open_url("https://example.com"))
            .font("minecraft:uniform");
        assert_eq!(component.to_plain_text(), "Hi There");
        assert_eq!(component.to_legacy(), "§c§lHi There");
    }

    #[test]
    fn invalid_codes_stay_literal() {
        assert_eq!(Component::from_legacy("a & b").to_plain_text(), "a & b");
        assert_eq!(Component::from_legacy("§z").to_plain_text(), "§z");
        assert_eq!(Component::from_legacy("§x§z").to_plain_text(), "§x§z");
    }
}
