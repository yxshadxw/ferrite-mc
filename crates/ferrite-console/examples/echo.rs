//! Interactive console demo.
//!
//! Run in a real terminal with:
//!
//! ```sh
//! cargo run -p ferrite-console --example echo
//! ```
//!
//! Type `help`, `say <text>`, `status` or `stop`; history, cursor keys and tab
//! completion work. A log line is printed every second to prove that output
//! does not corrupt the prompt. Lines are also written to `logs/latest.log`
//! without colors.

use std::sync::Arc;
use std::time::Duration;

use crossbeam::channel::RecvTimeoutError;
use ferrite_console::{Completer, Console, ConsoleConfig, ConsoleHandle, LogFile, LogLayer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn main() {
    let completer: Completer = Arc::new(|line: &str, _cursor: usize| {
        const COMMANDS: [&str; 5] = ["help", "say", "status", "tps", "stop"];
        let typed = line.trim();
        COMMANDS
            .iter()
            .filter(|command| command.starts_with(typed))
            .map(|command| (*command).to_string())
            .collect()
    });
    let (console, lines) = Console::start_with(ConsoleConfig {
        prompt: "ferrite> ".to_string(),
        history_path: Some("logs/console-history.txt".into()),
        completer: Some(completer),
        ..ConsoleConfig::default()
    });
    let file = LogFile::open("logs").expect("failed to open logs/latest.log");
    tracing_subscriber::registry()
        .with(LogLayer::new(console.output().clone()).with_file(file))
        .with(tracing_subscriber::filter::LevelFilter::INFO)
        .init();

    tracing::info!("Ferrite console example. Commands: say <text>, status, help, stop.");
    tracing::info!("§aA log line is printed every second while you type.");

    let (stop_ticker, ticker_stopped) = crossbeam::channel::unbounded::<()>();
    let ticker = std::thread::spawn(move || {
        let mut count = 0u32;
        while let Err(RecvTimeoutError::Timeout) =
            ticker_stopped.recv_timeout(Duration::from_secs(1))
        {
            count += 1;
            tracing::info!("§aheartbeat §7#{count} §8(prompt-safe log)");
        }
    });

    let shutdown = console.shutdown_receiver();
    loop {
        crossbeam::channel::select! {
            recv(lines) -> message => match message {
                Ok(line) => {
                    if !handle(&console, &line) {
                        break;
                    }
                }
                Err(_) => break,
            },
            recv(shutdown) -> reason => {
                if let Ok(reason) = reason {
                    tracing::warn!("console requested shutdown: {reason:?}");
                }
                break;
            }
        }
    }
    console.stop();
    tracing::info!("Goodbye.");
    let _ = stop_ticker.send(());
    let _ = ticker.join();
}

/// Returns `false` when the example should stop.
fn handle(console: &ConsoleHandle, line: &str) -> bool {
    match line {
        "help" => {
            console.print_line("commands: say <text>, status, tps, help, stop");
        }
        "status" => tracing::info!("§aonline§r: 0 players, protocol 777, §7regionized"),
        "tps" => tracing::info!("§atps§r: 20.0, §amst§r: 0.4 ms"),
        "stop" | "exit" | "quit" => {
            tracing::warn!("stop requested from the console");
            return false;
        }
        _ => match line.strip_prefix("say ") {
            Some(text) => tracing::info!("§e[Server] {text}"),
            None => tracing::info!("§7echo: §f{line}"),
        },
    }
    true
}
