//! Interactive terminal viewer.

pub mod app;
pub mod files;
pub mod images;
pub mod ui;

use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui_image::picker::Picker;

use crate::markdown::theme::Theme;
use app::{App, Initial, Mode};

/// Run the interactive viewer. Returns when the user quits.
pub fn run(initial: Initial, theme: Theme, watch: bool) -> Result<()> {
    let mut terminal = ratatui::init();
    let size = terminal.size()?;
    let picker = Picker::from_query_stdio().ok();
    let mut app = App::new(initial, theme, picker, watch);
    app.width = size.width;
    app.height = size.height;
    app.relayout();

    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
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
    }
    Ok(())
}
