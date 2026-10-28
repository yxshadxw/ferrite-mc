//! Prompt-aware console output.
//!
//! Every write goes through one mutex so log lines and redraws cannot
//! interleave. On a terminal a line is printed by clearing the prompt line,
//! writing the text, then repainting the prompt and the current input with the
//! cursor restored to its editing position.

use std::io::{self, Write};
use std::sync::Mutex;

use crossterm::cursor::MoveToColumn;
use crossterm::queue;
use crossterm::terminal::{Clear, ClearType};

use crate::color::{ansi_from_minecraft, strip_minecraft};

/// Shared console surface. Cloneable through an `Arc`.
pub struct ConsoleOutput {
    tty: bool,
    inner: Mutex<Inner>,
}

struct Inner {
    prompt: String,
    input: String,
    cursor: usize,
    /// Whether a prompt is currently managed on screen. Cleared on shutdown.
    active: bool,
}

impl ConsoleOutput {
    pub fn new(tty: bool, prompt: impl Into<String>) -> Self {
        Self {
            tty,
            inner: Mutex::new(Inner {
                prompt: prompt.into(),
                input: String::new(),
                cursor: 0,
                active: tty,
            }),
        }
    }

    pub fn is_tty(&self) -> bool {
        self.tty
    }

    pub fn set_prompt(&self, prompt: impl Into<String>) {
        {
            let mut inner = self.lock();
            inner.prompt = prompt.into();
        }
        self.redraw();
    }

    /// Print a line above the prompt. `§` codes are colored on a terminal and
    /// stripped when output is redirected.
    pub fn print_line(&self, line: &str) {
        let text = if self.tty {
            ansi_from_minecraft(line)
        } else {
            strip_minecraft(line)
        };
        let inner = self.lock();
        let mut out = io::stdout().lock();
        if self.tty && inner.active {
            let _ = queue!(out, MoveToColumn(0), Clear(ClearType::CurrentLine));
            for part in text.split('\n') {
                let _ = write!(out, "{part}\r\n");
            }
            let _ = write!(out, "{}{}", inner.prompt, inner.input);
            let _ = queue!(out, MoveToColumn(column(&inner)));
            let _ = out.flush();
        } else {
            for part in text.split('\n') {
                let _ = writeln!(out, "{part}");
            }
            let _ = out.flush();
        }
    }

    /// Replace the visible input and repaint the prompt line.
    pub fn update(&self, input: &str, cursor: usize) {
        {
            let mut inner = self.lock();
            inner.input.clear();
            inner.input.push_str(input);
            inner.cursor = cursor.min(input.chars().count());
            if !(self.tty && inner.active) {
                return;
            }
            paint(&inner);
        }
    }

    /// Repaint the current prompt and input.
    pub fn redraw(&self) {
        let inner = self.lock();
        if self.tty && inner.active {
            paint(&inner);
        }
    }

    /// Finish the current input line and start a fresh prompt.
    pub fn finish_line(&self) {
        let mut inner = self.lock();
        if !(self.tty && inner.active) {
            return;
        }
        inner.input.clear();
        inner.cursor = 0;
        let mut out = io::stdout().lock();
        let _ = write!(out, "\r\n{}{}", inner.prompt, inner.input);
        let _ = out.flush();
    }

    /// Clear the prompt line and stop managing it (used on shutdown).
    pub fn close(&self) {
        let mut inner = self.lock();
        if !(self.tty && inner.active) {
            inner.active = false;
            return;
        }
        inner.active = false;
        let mut out = io::stdout().lock();
        let _ = queue!(out, MoveToColumn(0), Clear(ClearType::CurrentLine));
        let _ = out.flush();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }
}

fn paint(inner: &Inner) {
    let mut out = io::stdout().lock();
    let _ = queue!(out, MoveToColumn(0), Clear(ClearType::CurrentLine));
    let _ = write!(out, "{}{}", inner.prompt, inner.input);
    let _ = queue!(out, MoveToColumn(column(inner)));
    let _ = out.flush();
}

fn column(inner: &Inner) -> u16 {
    let prompt = inner.prompt.chars().count();
    let cursor = inner.input.chars().take(inner.cursor).count();
    u16::try_from(prompt + cursor).unwrap_or(u16::MAX)
}
