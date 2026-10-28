//! Line editor state machine.
//!
//! The editor is terminal agnostic: callers translate raw input into [`Key`]
//! values and feed them to [`LineEditor::handle`]. The cursor is a character
//! index (not a byte offset), so multi-byte input never splits a character.

use crate::ShutdownReason;
use crate::history::{DEFAULT_HISTORY_LIMIT, History};

/// A normalized console key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Up,
    Down,
    CtrlA,
    CtrlC,
    CtrlD,
    CtrlE,
    CtrlU,
    CtrlW,
    Unknown,
}

/// What the console should do after a key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    /// Nothing visible changed.
    None,
    /// The input line needs repainting.
    Redraw,
    /// Tab was pressed; the caller should fetch candidates and call
    /// [`LineEditor::complete`].
    Complete,
    /// A complete command was submitted.
    Submit(String),
    /// The line editor is requesting shutdown.
    Shutdown(ShutdownReason),
    /// Candidates to print above the prompt.
    ShowCompletions(Vec<String>),
}

/// Interactive line editor with history.
pub struct LineEditor {
    buffer: String,
    cursor: usize,
    history: History,
    history_index: usize,
    draft: Option<String>,
    save_error: Option<String>,
}

impl LineEditor {
    pub fn new(history: History) -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history,
            history_index: 0,
            draft: None,
            save_error: None,
        }
    }

    /// An editor with an in-memory 1000 entry history.
    pub fn plain() -> Self {
        Self::new(History::new(DEFAULT_HISTORY_LIMIT))
    }

    pub fn text(&self) -> &str {
        &self.buffer
    }

    /// Cursor position as a character index.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// Take the last history persistence error, if any.
    pub fn take_error(&mut self) -> Option<String> {
        self.save_error.take()
    }

    /// Remember a command read outside the editor (piped stdin).
    pub fn remember(&mut self, line: &str) {
        if let Err(error) = self.history.record(line) {
            self.save_error = Some(error.to_string());
        }
    }

    /// Feed one key press without completion candidates.
    pub fn handle(&mut self, key: Key) -> EditorAction {
        match key {
            Key::Char(ch) => {
                self.insert(ch);
                EditorAction::Redraw
            }
            Key::Enter => self.submit(),
            Key::Tab => EditorAction::Complete,
            Key::Backspace => {
                if self.backspace() {
                    EditorAction::Redraw
                } else {
                    EditorAction::None
                }
            }
            Key::Delete => {
                if self.delete() {
                    EditorAction::Redraw
                } else {
                    EditorAction::None
                }
            }
            Key::Left => self.move_cursor(self.cursor.saturating_sub(1)),
            Key::Right => {
                let end = self.char_len();
                self.move_cursor((self.cursor + 1).min(end))
            }
            Key::Home | Key::CtrlA => self.move_cursor(0),
            Key::End | Key::CtrlE => {
                let end = self.char_len();
                self.move_cursor(end)
            }
            Key::Up => self.history_previous(),
            Key::Down => self.history_next(),
            Key::CtrlC => EditorAction::Shutdown(ShutdownReason::CtrlC),
            Key::CtrlD => {
                if self.buffer.is_empty() {
                    EditorAction::Shutdown(ShutdownReason::Eof)
                } else if self.delete() {
                    EditorAction::Redraw
                } else {
                    EditorAction::None
                }
            }
            Key::CtrlU => self.kill_to_start(),
            Key::CtrlW => {
                if self.delete_word() {
                    EditorAction::Redraw
                } else {
                    EditorAction::None
                }
            }
            Key::Unknown => EditorAction::None,
        }
    }

    /// Apply completion candidates for the word around the cursor.
    ///
    /// Candidates are whole replacement words. A single candidate replaces the
    /// word; several candidates extend it to their longest common prefix and
    /// are listed without changing it when that prefix is already typed.
    pub fn complete(&mut self, candidates: Vec<String>) -> EditorAction {
        if candidates.is_empty() {
            return EditorAction::None;
        }
        let (start, end) = self.word_bounds();
        if candidates.len() == 1 {
            self.replace_chars(start, end, &candidates[0]);
            return EditorAction::Redraw;
        }
        let prefix = common_prefix(&candidates);
        let current: String = self
            .buffer
            .chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect();
        if prefix == current {
            return EditorAction::ShowCompletions(candidates);
        }
        self.replace_chars(start, end, &prefix);
        EditorAction::Redraw
    }

    fn submit(&mut self) -> EditorAction {
        let line = self.buffer.trim().to_string();
        self.buffer.clear();
        self.cursor = 0;
        self.history_index = 0;
        self.draft = None;
        if line.is_empty() {
            return EditorAction::Redraw;
        }
        if let Err(error) = self.history.record(&line) {
            self.save_error = Some(error.to_string());
        }
        EditorAction::Submit(line)
    }

    fn char_len(&self) -> usize {
        self.buffer.chars().count()
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_index)
            .map_or(self.buffer.len(), |(byte, _)| byte)
    }

    fn insert(&mut self, ch: char) {
        let at = self.byte_index(self.cursor);
        self.buffer.insert(at, ch);
        self.cursor += 1;
    }

    fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let start = self.byte_index(self.cursor - 1);
        let end = self.byte_index(self.cursor);
        self.buffer.replace_range(start..end, "");
        self.cursor -= 1;
        true
    }

    fn delete(&mut self) -> bool {
        if self.cursor >= self.char_len() {
            return false;
        }
        let start = self.byte_index(self.cursor);
        let end = self.byte_index(self.cursor + 1);
        self.buffer.replace_range(start..end, "");
        true
    }

    fn move_cursor(&mut self, target: usize) -> EditorAction {
        if target == self.cursor {
            EditorAction::None
        } else {
            self.cursor = target;
            EditorAction::Redraw
        }
    }

    fn kill_to_start(&mut self) -> EditorAction {
        if self.cursor == 0 {
            return EditorAction::None;
        }
        let end = self.byte_index(self.cursor);
        self.buffer.replace_range(0..end, "");
        self.cursor = 0;
        EditorAction::Redraw
    }

    fn delete_word(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let chars: Vec<char> = self.buffer.chars().collect();
        let mut start = self.cursor;
        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let from = self.byte_index(start);
        let to = self.byte_index(self.cursor);
        self.buffer.replace_range(from..to, "");
        self.cursor = start;
        true
    }

    fn history_previous(&mut self) -> EditorAction {
        if self.history_index >= self.history.len() {
            return EditorAction::None;
        }
        if self.history_index == 0 {
            self.draft = Some(self.buffer.clone());
        }
        self.history_index += 1;
        self.buffer = self
            .history
            .get(self.history_index - 1)
            .unwrap_or_default()
            .to_string();
        self.cursor = self.char_len();
        EditorAction::Redraw
    }

    fn history_next(&mut self) -> EditorAction {
        if self.history_index == 0 {
            return EditorAction::None;
        }
        self.history_index -= 1;
        self.buffer = if self.history_index == 0 {
            self.draft.take().unwrap_or_default()
        } else {
            self.history
                .get(self.history_index - 1)
                .unwrap_or_default()
                .to_string()
        };
        self.cursor = self.char_len();
        EditorAction::Redraw
    }

    /// Character range of the whitespace-delimited word around the cursor.
    fn word_bounds(&self) -> (usize, usize) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let mut start = self.cursor.min(chars.len());
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let mut end = self.cursor.min(chars.len());
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }
        (start, end)
    }

    fn replace_chars(&mut self, start: usize, end: usize, text: &str) {
        let from = self.byte_index(start);
        let to = self.byte_index(end);
        self.buffer.replace_range(from..to, text);
        self.cursor = start + text.chars().count();
    }
}

fn common_prefix(candidates: &[String]) -> String {
    let Some(first) = candidates.first() else {
        return String::new();
    };
    let first: Vec<char> = first.chars().collect();
    let mut length = first.len();
    for candidate in &candidates[1..] {
        let shared = first
            .iter()
            .zip(candidate.chars())
            .take_while(|(a, b)| *a == b)
            .count();
        length = length.min(shared);
    }
    first[..length].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> LineEditor {
        let mut editor = LineEditor::plain();
        for ch in text.chars() {
            editor.handle(Key::Char(ch));
        }
        editor
    }

    fn press(editor: &mut LineEditor, keys: &[Key]) -> EditorAction {
        let mut last = EditorAction::None;
        for key in keys {
            last = editor.handle(*key);
        }
        last
    }

    #[test]
    fn inserts_and_moves() {
        let mut editor = typed("helo");
        assert_eq!(editor.text(), "helo");
        assert_eq!(editor.cursor(), 4);
        editor.handle(Key::Left);
        assert_eq!(editor.cursor(), 3);
        editor.handle(Key::Char('l'));
        assert_eq!(editor.text(), "hello");
        assert_eq!(editor.cursor(), 4);
        press(&mut editor, &[Key::Home]);
        assert_eq!(editor.cursor(), 0);
        press(&mut editor, &[Key::Right, Key::Right]);
        assert_eq!(editor.cursor(), 2);
        press(&mut editor, &[Key::End]);
        assert_eq!(editor.cursor(), 5);
        press(&mut editor, &[Key::CtrlA]);
        assert_eq!(editor.cursor(), 0);
        press(&mut editor, &[Key::CtrlE]);
        assert_eq!(editor.cursor(), 5);
    }

    #[test]
    fn edit_keys() {
        let mut editor = typed("hellp");
        editor.handle(Key::Left);
        editor.handle(Key::Right);
        editor.handle(Key::Backspace);
        assert_eq!(editor.text(), "hell");
        press(&mut editor, &[Key::Home, Key::Delete]);
        assert_eq!(editor.text(), "ell");
        assert_eq!(editor.cursor(), 0);
        press(&mut editor, &[Key::End]);
        press(&mut editor, &[Key::CtrlU]);
        assert_eq!(editor.text(), "");
        assert_eq!(editor.cursor(), 0);
    }

    #[test]
    fn ctrl_w_deletes_previous_word() {
        let mut editor = typed("say hello world");
        press(&mut editor, &[Key::CtrlW]);
        assert_eq!(editor.text(), "say hello ");
        press(&mut editor, &[Key::CtrlW]);
        assert_eq!(editor.text(), "say ");
        press(&mut editor, &[Key::CtrlW, Key::CtrlW]);
        assert_eq!(editor.text(), "");
        assert_eq!(editor.cursor(), 0);
    }

    #[test]
    fn submit_trims_and_clears() {
        let mut editor = typed("  say hi  ");
        let action = editor.handle(Key::Enter);
        assert_eq!(action, EditorAction::Submit("say hi".to_string()));
        assert_eq!(editor.text(), "");
        assert_eq!(editor.cursor(), 0);
        assert_eq!(editor.history().len(), 1);
        assert_eq!(editor.history().get(0), Some("say hi"));
        assert_eq!(editor.handle(Key::Enter), EditorAction::Redraw);
        assert_eq!(editor.history().len(), 1);
    }

    #[test]
    fn history_navigation_keeps_draft() {
        let mut editor = LineEditor::new(History::new(10));
        editor.remember("first");
        editor.remember("second");
        for ch in "unfinished".chars() {
            editor.handle(Key::Char(ch));
        }
        press(&mut editor, &[Key::Up]);
        assert_eq!(editor.text(), "second");
        press(&mut editor, &[Key::Up]);
        assert_eq!(editor.text(), "first");
        press(&mut editor, &[Key::Up]);
        assert_eq!(editor.text(), "first");
        press(&mut editor, &[Key::Down]);
        assert_eq!(editor.text(), "second");
        press(&mut editor, &[Key::Down]);
        assert_eq!(editor.text(), "unfinished");
        press(&mut editor, &[Key::Down]);
        assert_eq!(editor.text(), "unfinished");
    }

    #[test]
    fn shutdown_keys() {
        let mut editor = LineEditor::plain();
        assert_eq!(
            editor.handle(Key::CtrlC),
            EditorAction::Shutdown(ShutdownReason::CtrlC)
        );
        assert_eq!(
            editor.handle(Key::CtrlD),
            EditorAction::Shutdown(ShutdownReason::Eof)
        );
        let mut editor = typed("abc");
        assert_eq!(editor.handle(Key::CtrlD), EditorAction::None);
        assert_eq!(editor.text(), "abc");
        press(&mut editor, &[Key::CtrlA]);
        assert_eq!(editor.handle(Key::CtrlD), EditorAction::Redraw);
        assert_eq!(editor.text(), "bc");
    }

    #[test]
    fn unicode_cursor_moves_by_character() {
        let mut editor = typed("aéü中");
        assert_eq!(editor.cursor(), 4);
        press(&mut editor, &[Key::Left]);
        editor.handle(Key::Backspace);
        assert_eq!(editor.text(), "aé中");
        assert_eq!(editor.cursor(), 2);
    }

    #[test]
    fn completion_replaces_word_and_extends_prefix() {
        let mut editor = typed("gi");
        let action = editor.complete(vec!["give".into()]);
        assert_eq!(action, EditorAction::Redraw);
        assert_eq!(editor.text(), "give");
        assert_eq!(editor.cursor(), 4);

        let mut editor = typed("say he");
        let action = editor.complete(vec!["hello".into(), "help".into()]);
        assert_eq!(action, EditorAction::Redraw);
        assert_eq!(editor.text(), "say hel");
        assert_eq!(editor.cursor(), 7);

        let action = editor.complete(vec!["hello".into(), "help".into()]);
        assert_eq!(
            action,
            EditorAction::ShowCompletions(vec!["hello".into(), "help".into()])
        );
        assert_eq!(editor.text(), "say hel");

        let mut editor = typed("");
        assert_eq!(editor.complete(vec![]), EditorAction::None);
    }

    #[test]
    fn tab_requests_completion() {
        let mut editor = typed("sa");
        assert_eq!(editor.handle(Key::Tab), EditorAction::Complete);
        assert_eq!(editor.text(), "sa");
    }
}
