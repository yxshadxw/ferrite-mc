//! Console construction and the stdin reader thread.
//!
//! [`Console::start`] never blocks the caller: a dedicated thread owns stdin,
//! performs line editing when it is a terminal, and forwards complete command
//! lines through a channel. Shutdown requests (Ctrl+C, Ctrl+D, or an explicit
//! [`ConsoleHandle::stop`]) arrive on a second, cloneable channel.

use std::io::{self, BufRead, IsTerminal};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crossbeam::channel::{Receiver, Sender, unbounded};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal;

use crate::ShutdownReason;
use crate::editor::{EditorAction, Key, LineEditor};
use crate::history::{DEFAULT_HISTORY_LIMIT, History};
use crate::output::ConsoleOutput;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Callback for tab completion: receives the current input and the cursor as a
/// character index, and returns whole replacement words.
pub type Completer = Arc<dyn Fn(&str, usize) -> Vec<String> + Send + Sync>;

/// Console configuration.
#[derive(Clone)]
pub struct ConsoleConfig {
    pub prompt: String,
    /// File used to persist command history. `None` keeps it in memory only.
    pub history_path: Option<PathBuf>,
    /// Maximum remembered commands (default 1000).
    pub history_limit: usize,
    pub completer: Option<Completer>,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            prompt: "> ".to_string(),
            history_path: None,
            history_limit: DEFAULT_HISTORY_LIMIT,
            completer: None,
        }
    }
}

/// Factory for the interactive console.
pub struct Console;

impl Console {
    /// Start reading commands with `prompt`. Equivalent to
    /// [`Console::start_with`] with default configuration.
    pub fn start(prompt: impl Into<String>) -> (ConsoleHandle, Receiver<String>) {
        Self::start_with(ConsoleConfig {
            prompt: prompt.into(),
            ..ConsoleConfig::default()
        })
    }

    /// Start reading commands. The returned receiver yields complete, trimmed
    /// command lines; the handle owns shutdown signalling and prompt-safe
    /// output.
    pub fn start_with(config: ConsoleConfig) -> (ConsoleHandle, Receiver<String>) {
        let tty = io::stdin().is_terminal() && io::stdout().is_terminal();
        let output = Arc::new(ConsoleOutput::new(tty, config.prompt.clone()));
        let (line_sender, line_receiver) = unbounded();
        let (shutdown_sender, shutdown_receiver) = unbounded();
        let stopped = Arc::new(AtomicBool::new(false));
        let raw = Arc::new(RawMode::new());
        let handle = ConsoleHandle {
            output: Arc::clone(&output),
            stopped: Arc::clone(&stopped),
            raw: Arc::clone(&raw),
            shutdown_sender: shutdown_sender.clone(),
            shutdown_receiver,
        };
        thread::Builder::new()
            .name("ferrite-console".to_string())
            .spawn(move || {
                run(config, output, line_sender, shutdown_sender, stopped, raw);
            })
            .expect("failed to spawn the console thread");
        (handle, line_receiver)
    }
}

/// Handle to a running console.
pub struct ConsoleHandle {
    output: Arc<ConsoleOutput>,
    stopped: Arc<AtomicBool>,
    raw: Arc<RawMode>,
    shutdown_sender: Sender<ShutdownReason>,
    shutdown_receiver: Receiver<ShutdownReason>,
}

impl ConsoleHandle {
    /// Prompt-aware output shared with [`crate::LogLayer`].
    pub fn output(&self) -> &Arc<ConsoleOutput> {
        &self.output
    }

    pub fn is_tty(&self) -> bool {
        self.output.is_tty()
    }

    /// Print a line above the prompt without corrupting it.
    pub fn print_line(&self, line: &str) {
        self.output.print_line(line);
    }

    /// Replace the prompt shown after each submitted line.
    pub fn set_prompt(&self, prompt: impl Into<String>) {
        self.output.set_prompt(prompt);
    }

    /// Cloneable stream of shutdown requests. Receives exactly one reason per
    /// console lifetime.
    pub fn shutdown_receiver(&self) -> Receiver<ShutdownReason> {
        self.shutdown_receiver.clone()
    }

    /// Ask the reader thread to stop, restore the terminal and emit
    /// [`ShutdownReason::Requested`]. Safe to call more than once.
    pub fn stop(&self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        self.raw.disable();
        self.output.close();
        let _ = self.shutdown_sender.send(ShutdownReason::Requested);
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

impl Drop for ConsoleHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

struct RawMode {
    enabled: AtomicBool,
}

impl RawMode {
    fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
        }
    }

    fn enable(&self) -> bool {
        if terminal::enable_raw_mode().is_ok() {
            self.enabled.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    fn disable(&self) {
        if self.enabled.swap(false, Ordering::SeqCst) {
            let _ = terminal::disable_raw_mode();
        }
    }
}

fn run(
    config: ConsoleConfig,
    output: Arc<ConsoleOutput>,
    lines: Sender<String>,
    shutdown: Sender<ShutdownReason>,
    stopped: Arc<AtomicBool>,
    raw: Arc<RawMode>,
) {
    let history = match &config.history_path {
        Some(path) => match History::load(path, config.history_limit) {
            Ok(history) => history,
            Err(error) => {
                output.print_line(&format!("Failed to read console history: {error}"));
                History::new(config.history_limit)
            }
        },
        None => History::new(config.history_limit),
    };
    let mut editor = LineEditor::new(history);
    if output.is_tty() && raw.enable() {
        output.redraw();
        let error = tty_loop(
            &config,
            &output,
            &mut editor,
            &lines,
            &shutdown,
            &stopped,
            &raw,
        );
        raw.disable();
        if let Some(error) = error {
            output.close();
            if !stopped.load(Ordering::SeqCst) {
                output.print_line(&format!("Console input stopped: {error}"));
                let _ = shutdown.send(ShutdownReason::Eof);
            }
        }
        output.close();
    } else {
        output.close();
        plain_loop(&output, &mut editor, &lines, &shutdown, &stopped);
    }
}

fn plain_loop(
    output: &ConsoleOutput,
    editor: &mut LineEditor,
    lines: &Sender<String>,
    shutdown: &Sender<ShutdownReason>,
    stopped: &AtomicBool,
) {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut buffer = String::new();
    loop {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        buffer.clear();
        match input.read_line(&mut buffer) {
            Ok(0) => {
                if !stopped.load(Ordering::SeqCst) {
                    let _ = shutdown.send(ShutdownReason::Eof);
                }
                return;
            }
            Ok(_) => {
                let line = buffer.trim_end_matches(['\r', '\n']).to_string();
                if line.trim().is_empty() {
                    continue;
                }
                editor.remember(&line);
                if let Some(error) = editor.take_error() {
                    output.print_line(&format!("Failed to save console history: {error}"));
                }
                if lines.send(line).is_err() {
                    return;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                let _ = shutdown.send(ShutdownReason::Eof);
                return;
            }
        }
    }
}

fn tty_loop(
    config: &ConsoleConfig,
    output: &ConsoleOutput,
    editor: &mut LineEditor,
    lines: &Sender<String>,
    shutdown: &Sender<ShutdownReason>,
    stopped: &AtomicBool,
    raw: &RawMode,
) -> Option<io::Error> {
    loop {
        if stopped.load(Ordering::SeqCst) {
            return None;
        }
        match event::poll(POLL_INTERVAL) {
            Ok(false) => continue,
            Ok(true) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Some(error),
        }
        let event = match event::read() {
            Ok(event) => event,
            Err(error) => return Some(error),
        };
        if stopped.load(Ordering::SeqCst) {
            return None;
        }
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if handle_key(config, output, editor, lines, shutdown, raw, key) {
                    return None;
                }
            }
            Event::Paste(text) => {
                for ch in text.chars() {
                    editor.handle(Key::Char(ch));
                }
                output.update(editor.text(), editor.cursor());
            }
            Event::Resize(_, _) => output.redraw(),
            _ => {}
        }
    }
}

/// Returns `true` when the console should stop.
fn handle_key(
    config: &ConsoleConfig,
    output: &ConsoleOutput,
    editor: &mut LineEditor,
    lines: &Sender<String>,
    shutdown: &Sender<ShutdownReason>,
    raw: &RawMode,
    key_event: KeyEvent,
) -> bool {
    let key = map_key(key_event);
    let action = if key == Key::Tab {
        let candidates = config.completer.as_ref().map_or_else(Vec::new, |complete| {
            complete(editor.text(), editor.cursor())
        });
        editor.complete(candidates)
    } else {
        editor.handle(key)
    };
    match action {
        EditorAction::None | EditorAction::Complete => false,
        EditorAction::Redraw => {
            output.update(editor.text(), editor.cursor());
            false
        }
        EditorAction::ShowCompletions(candidates) => {
            output.print_line(&candidates.join("  "));
            output.redraw();
            false
        }
        EditorAction::Submit(line) => {
            output.finish_line();
            if let Some(error) = editor.take_error() {
                output.print_line(&format!("Failed to save console history: {error}"));
            }
            lines.send(line).is_err()
        }
        EditorAction::Shutdown(reason) => {
            output.close();
            // Restore cooked mode before waking the server so its final log
            // lines use normal newline translation.
            raw.disable();
            let _ = shutdown.send(reason);
            true
        }
    }
}

fn map_key(event: KeyEvent) -> Key {
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        return match event.code {
            KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
                'a' => Key::CtrlA,
                'c' => Key::CtrlC,
                'd' => Key::CtrlD,
                'e' => Key::CtrlE,
                'u' => Key::CtrlU,
                'w' => Key::CtrlW,
                _ => Key::Unknown,
            },
            _ => Key::Unknown,
        };
    }
    match event.code {
        KeyCode::Char(ch) => Key::Char(ch),
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        _ => Key::Unknown,
    }
}
