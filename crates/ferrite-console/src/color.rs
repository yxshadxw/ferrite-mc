//! Minecraft `§` formatting codes.
//!
//! The console renders codes as ANSI escapes; the log file strips them. Codes
//! that have no useful terminal equivalent (`§k` obfuscated, unknown codes and
//! a trailing `§`) are removed.

/// Translate Minecraft `§` codes into ANSI escape sequences.
pub fn ansi_from_minecraft(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            if let Some(code) = chars.next() {
                push_ansi(code, &mut out);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Remove Minecraft `§` codes, leaving plain text.
pub fn strip_minecraft(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            chars.next();
        } else {
            out.push(ch);
        }
    }
    out
}

/// Color codes reset formatting first, like the vanilla client.
fn push_ansi(code: char, out: &mut String) {
    let (reset, value) = match code.to_ascii_lowercase() {
        '0' => (true, 30),
        '1' => (true, 34),
        '2' => (true, 32),
        '3' => (true, 36),
        '4' => (true, 31),
        '5' => (true, 35),
        '6' => (true, 33),
        '7' => (true, 37),
        '8' => (true, 90),
        '9' => (true, 94),
        'a' => (true, 92),
        'b' => (true, 96),
        'c' => (true, 91),
        'd' => (true, 95),
        'e' => (true, 93),
        'f' => (true, 97),
        'l' => (false, 1),
        'm' => (false, 9),
        'n' => (false, 4),
        'o' => (false, 3),
        'r' => {
            out.push_str("\x1b[0m");
            return;
        }
        'k' => return,
        _ => return,
    };
    if reset {
        out.push_str("\x1b[0;");
        out.push_str(&value.to_string());
        out.push('m');
    } else {
        out.push_str("\x1b[");
        out.push_str(&value.to_string());
        out.push('m');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_formatting_become_ansi() {
        assert_eq!(
            ansi_from_minecraft("§aHello §lBold§r!"),
            "\x1b[0;92mHello \x1b[1mBold\x1b[0m!"
        );
        assert_eq!(ansi_from_minecraft("§0black"), "\x1b[0;30mblack");
        assert_eq!(ansi_from_minecraft("§fwhite"), "\x1b[0;97mwhite");
        assert_eq!(ansi_from_minecraft("§mstruck"), "\x1b[9mstruck");
        assert_eq!(ansi_from_minecraft("§nunder"), "\x1b[4munder");
        assert_eq!(ansi_from_minecraft("§oitalic"), "\x1b[3mitalic");
    }

    #[test]
    fn unknown_obfuscated_and_trailing_codes_are_dropped() {
        assert_eq!(ansi_from_minecraft("a§kb"), "ab");
        assert_eq!(ansi_from_minecraft("a§zb"), "ab");
        assert_eq!(ansi_from_minecraft("dangling §"), "dangling ");
        assert_eq!(strip_minecraft("dangling §"), "dangling ");
    }

    #[test]
    fn stripping_removes_codes_only() {
        assert_eq!(strip_minecraft("§aHello §lBold§r!"), "Hello Bold!");
        assert_eq!(strip_minecraft("no codes"), "no codes");
        assert_eq!(strip_minecraft("§c§l"), "");
    }
}
