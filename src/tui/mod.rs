//! Interactive terminal viewer.

pub mod app;
pub mod files;
pub mod images;
pub mod ui;

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui_image::picker::Picker;

use crate::markdown::theme::Theme;
use crate::narration;
use app::{App, Initial, Mode};

/// How long to wait for the terminal's answer to a device-status query.
const TERMINAL_PROBE: Duration = Duration::from_millis(350);

/// Run the interactive viewer. Returns when the user quits.
pub fn run(
    initial: Initial,
    theme: Theme,
    watch: bool,
    narration_config: narration::Config,
    start_reading: bool,
) -> Result<()> {
    let mut terminal = ratatui::init();
    let size = terminal.size()?;
    let picker = picker();
    let mut app = App::new(initial, theme, picker, watch, narration_config);
    app.width = size.width;
    app.height = size.height;
    app.relayout();
    if start_reading {
        app.start_narration();
    }

    let result = event_loop(&mut terminal, &mut app);
    app.shutdown();
    ratatui::restore();
    result
}

/// Build the image picker.
///
/// `Picker::from_query_stdio` asks the terminal for its graphics capabilities
/// on a background thread that keeps reading stdin until it gets an answer.
/// A terminal that never answers (a bare pty, some CI shells) leaves that
/// thread racing the event loop for keystrokes, which silently swallows
/// input, so only query a terminal that has proven it replies; otherwise fall
/// back to half-blocks.
fn picker() -> Option<Picker> {
    if !io::stdin().is_terminal() || !terminal_answers() {
        return Some(Picker::halfblocks());
    }
    Some(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()))
}

/// Ask for a device status report (`CSI 5 n`) and report whether the terminal
/// answered within [`TERMINAL_PROBE`]. The reply is drained so the event loop
/// starts with clean input. Must run with raw mode already enabled.
fn terminal_answers() -> bool {
    let mut stdout = io::stdout();
    if stdout
        .write_all(b"\x1b[5n")
        .and_then(|_| stdout.flush())
        .is_err()
    {
        return false;
    }
    let deadline = Instant::now() + TERMINAL_PROBE;
    let mut answered = false;
    while !answered {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        answered = event::poll(left).unwrap_or(false);
    }
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let _ = event::read();
    }
    answered
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    while !app.quit {
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => app.handle_key(key),
                Event::Resize(width, height) => {
                    app.width = width;
                    app.height = height;
                    app.relayout();
                }
                Event::Paste(text) if app.mode == Mode::Search => {
                    app.search.query.push_str(&text);
                    app.recompute_matches();
                }
                _ => {}
            }
        }
        app.poll_watcher();
        app.poll_narration();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui_image::picker::ProtocolType;

    /// The terminal query thread reads stdin; it must never be started when
    /// there is no terminal on the other end to answer it.
    #[test]
    fn picker_does_not_query_a_non_terminal_stdin() {
        assert!(!io::stdin().is_terminal(), "cargo test pipes stdin");
        let picker = picker().expect("fallback picker");
        assert_eq!(picker.protocol_type(), ProtocolType::Halfblocks);
    }
}
