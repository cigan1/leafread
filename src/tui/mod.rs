//! Interactive terminal viewer.

pub mod app;
pub mod files;
pub mod images;
pub mod ui;

use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui_image::picker::Picker;
use ratatui_image::picker::ProtocolType;
use ratatui_image::picker::cap_parser::QueryStdioOptions;

use crate::markdown::theme::Theme;
use crate::narration;
use crate::terminal;
use app::{App, Initial, Mode};

/// Run the interactive viewer. Returns when the user quits.
pub fn run(
    initial: Initial,
    theme: Theme,
    watch: bool,
    narration_config: narration::Config,
    start_reading: bool,
) -> Result<()> {
    let picker = picker();
    terminal::reclaim_raw_mode();
    let mut terminal = ratatui::init();
    let size = terminal.size()?;
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
    if !terminal::answers() {
        return Some(Picker::halfblocks());
    }
    let mut options = QueryStdioOptions::default();
    if !terminal::kitty_graphics_terminal() {
        // The kitty query carries a visible payload. A terminal that does not
        // implement the protocol prints it instead of answering, and it lands
        // on the main screen, where it outlives the viewer. Ask only terminals
        // known to answer it silently; the rest still report sixel, font size
        // and iTerm2 support.
        options.blacklist_protocols.push(ProtocolType::Kitty);
    }
    Some(Picker::from_query_stdio_with_options(options).unwrap_or_else(|_| Picker::halfblocks()))
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
        assert!(
            !std::io::IsTerminal::is_terminal(&std::io::stdin()),
            "cargo test pipes stdin"
        );
        let picker = picker().expect("fallback picker");
        assert_eq!(picker.protocol_type(), ProtocolType::Halfblocks);
    }
}
