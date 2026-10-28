//! Interactive server console: prompt-safe logging, line editing and history.
//!
//! [`Console::start`] reads command lines on a dedicated thread and delivers
//! them through a channel, so the game loop never blocks on stdin. On a
//! terminal, input is read in raw mode with history, cursor keys, `Ctrl+A/E/U/W`
//! and tab completion. Piped input falls back to plain line reading.
//!
//! [`LogLayer`] is a `tracing_subscriber` layer that clears the prompt line,
//! prints `[HH:MM:SS LEVEL] message` and redraws the prompt with the current
//! input. It can mirror the same lines, without colors or `§` codes, to a
//! rotating `logs/latest.log`.

mod color;
mod console;
mod editor;
mod history;
mod log;
mod output;

pub use color::{ansi_from_minecraft, strip_minecraft};
pub use console::{Completer, Console, ConsoleConfig, ConsoleHandle};
pub use editor::{EditorAction, Key, LineEditor};
pub use history::{DEFAULT_HISTORY_LIMIT, History};
pub use log::{LogFile, LogLayer, archive_path};
pub use output::ConsoleOutput;

/// Why the console stopped reading input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownReason {
    /// `Ctrl+C` was pressed.
    CtrlC,
    /// `Ctrl+D` was pressed on an empty line, or stdin reached EOF.
    Eof,
    /// Shutdown was requested programmatically through the handle.
    Requested,
}
