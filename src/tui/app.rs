//! Interactive viewer state and event handling.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_image::picker::Picker;

use crate::markdown::layout::{self, LayoutOptions, Rendered};
use crate::markdown::model::Document;
use crate::markdown::parser;
use crate::markdown::syntax::Highlighter;
use crate::markdown::theme::Theme;
use crate::narration::{self, Narration};
use crate::tui::files::FileBrowser;
use crate::tui::images::ImageCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Toc,
    Links,
    Help,
}

pub struct SearchState {
    pub query: String,
    pub saved: String,
    pub matches: Vec<usize>,
    pub current: usize,
}

impl SearchState {
    fn new() -> Self {
        Self {
            query: String::new(),
            saved: String::new(),
            matches: Vec::new(),
            current: 0,
        }
    }
}

pub struct Initial {
    pub path: Option<PathBuf>,
    pub raw: String,
    pub directory: Option<PathBuf>,
}

pub struct App {
    pub theme: Theme,
    pub highlighter: Highlighter,
    pub path: Option<PathBuf>,
    pub base_dir: PathBuf,
    pub raw: String,
    pub doc: Document,
    pub rendered: Rendered,
    pub width: u16,
    pub height: u16,
    pub scroll: usize,
    pub mode: Mode,
    pub search: SearchState,
    pub toc_selected: usize,
    pub link_selected: Option<usize>,
    pub files: Option<FileBrowser>,
    pub images: ImageCache,
    pub narration: Narration,
    narration_config: narration::Config,
    pub watch: bool,
    watcher: Option<RecommendedWatcher>,
    watch_rx: Option<Receiver<()>>,
    pub status: Option<(String, Instant)>,
    pub quit: bool,
    pub show_front_matter: bool,
}

impl App {
    pub fn new(
        initial: Initial,
        theme: Theme,
        picker: Option<Picker>,
        watch: bool,
        narration_config: narration::Config,
    ) -> Self {
        let highlighter = Highlighter::new(theme.syntax_theme, theme.text);
        let mut app = Self {
            theme,
            highlighter,
            path: None,
            base_dir: initial
                .directory
                .clone()
                .or_else(|| {
                    initial
                        .path
                        .as_ref()
                        .and_then(|p| p.parent().map(Path::to_path_buf))
                })
                .unwrap_or_else(|| PathBuf::from(".")),
            raw: String::new(),
            doc: Document::default(),
            rendered: Rendered::default(),
            width: 80,
            height: 24,
            scroll: 0,
            mode: Mode::Normal,
            search: SearchState::new(),
            toc_selected: 0,
            link_selected: None,
            files: None,
            images: ImageCache::new(picker),
            narration: Narration::new(),
            narration_config,
            watch,
            watcher: None,
            watch_rx: None,
            status: None,
            quit: false,
            show_front_matter: false,
        };
        if let Some(dir) = &initial.directory {
            app.files = Some(FileBrowser::new(dir));
        } else {
            app.raw = initial.raw.clone();
            app.doc = parser::parse(&initial.raw);
            app.path = initial.path.clone();
            app.relayout();
            if watch {
                app.setup_watcher();
            }
        }
        app
    }

    pub fn viewport_height(&self) -> usize {
        self.height.saturating_sub(2).max(1) as usize
    }

    pub fn content_width(&self) -> usize {
        self.width.saturating_sub(2).max(8) as usize
    }

    pub fn max_scroll(&self) -> usize {
        self.rendered
            .lines
            .len()
            .saturating_sub(self.viewport_height())
    }

    /// Re-render at the current width. Returns true when a running narration
    /// had to stop because the document text changed under it.
    pub fn relayout(&mut self) -> bool {
        let options = LayoutOptions {
            width: self.content_width(),
            theme: &self.theme,
            highlighter: &self.highlighter,
            show_front_matter: self.show_front_matter,
            reserve_image_rows: true,
        };
        let mut rendered = layout::render(&self.doc, &options);
        if self.show_front_matter {
            rendered.title = rendered.title.or(self.doc.front_matter.as_ref().map(|_| {
                self.path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "stdin".to_string())
            }));
        }
        self.rendered = rendered;
        self.images.clear();
        self.recompute_matches();
        self.clamp_scroll();
        self.toc_selected = self
            .toc_selected
            .min(self.rendered.toc.len().saturating_sub(1));
        if self.narration.active() && !self.narration.matches(&self.rendered.words) {
            self.narration.stop();
            self.set_status("read aloud stopped (document changed)".into());
            return true;
        }
        false
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }

    pub fn recompute_matches(&mut self) {
        self.search.matches.clear();
        if self.search.query.is_empty() {
            return;
        }
        let needle = self.search.query.to_lowercase();
        for (index, line) in self.rendered.lines.iter().enumerate() {
            let text = line_text(line).to_lowercase();
            if text.contains(&needle) {
                self.search.matches.push(index);
            }
        }
        if self.search.matches.is_empty() {
            self.search.current = 0;
        } else {
            self.search.current = self.search.current.min(self.search.matches.len() - 1);
        }
    }

    pub fn scroll_to(&mut self, line: usize) {
        self.scroll = line.min(self.max_scroll());
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let next = self.scroll as isize + delta;
        self.scroll = next.clamp(0, self.max_scroll() as isize) as usize;
    }

    pub fn scroll_half_page(&mut self, down: bool) {
        let delta = (self.viewport_height() / 2).max(1) as isize;
        self.scroll_by(if down { delta } else { -delta });
    }

    pub fn scroll_page(&mut self, down: bool) {
        let delta = self.viewport_height().max(1) as isize;
        self.scroll_by(if down { delta } else { -delta });
    }

    pub fn load_path(&mut self, path: &Path) -> std::io::Result<()> {
        let raw = std::fs::read_to_string(path)?;
        self.narration.stop();
        self.raw = raw;
        self.doc = parser::parse(&self.raw);
        self.path = Some(path.to_path_buf());
        self.base_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        self.scroll = 0;
        self.link_selected = None;
        self.toc_selected = 0;
        self.files = None;
        self.relayout();
        if self.watch {
            self.setup_watcher();
        }
        self.set_status(format!("opened {}", path.display()));
        Ok(())
    }

    fn reload(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return;
        };
        if raw == self.raw {
            return;
        }
        let fraction = if self.max_scroll() == 0 {
            0.0
        } else {
            self.scroll as f64 / self.max_scroll() as f64
        };
        self.raw = raw;
        self.doc = parser::parse(&self.raw);
        let stopped_reading = self.relayout();
        self.scroll = (fraction * self.max_scroll() as f64).round() as usize;
        self.clamp_scroll();
        self.set_status(if stopped_reading {
            "reloaded (read aloud stopped)".to_string()
        } else {
            "reloaded (file changed)".to_string()
        });
    }

    pub fn set_status(&mut self, message: String) {
        self.status = Some((message, Instant::now()));
    }

    /// Start speaking the document from the current scroll position.
    pub fn start_narration(&mut self) {
        if self.rendered.words.is_empty() {
            self.set_status("nothing to read aloud here".into());
            return;
        }
        match self
            .narration
            .start(&self.rendered.words, self.scroll, &self.narration_config)
        {
            // The status bar shows the narration state itself, including the
            // engine while speech is being prepared — no transient message.
            Ok(()) => {}
            Err(message) => self.set_status(message),
        }
    }

    /// `p`: start, pause, or resume narration.
    pub fn toggle_narration(&mut self) {
        match self.narration.state() {
            narration::State::Playing | narration::State::Paused => self.narration.toggle_pause(),
            narration::State::Preparing => self.set_status("still preparing speech…".into()),
            narration::State::Idle => self.start_narration(),
        }
    }

    /// `s` (or esc): stop narration and silence the audio.
    pub fn stop_narration(&mut self) {
        if self.narration.active() {
            self.narration.stop();
            self.set_status("read aloud stopped".into());
        }
    }

    /// Stop all background work; called when the viewer exits.
    pub fn shutdown(&mut self) {
        self.narration.stop();
    }

    /// Pick up narration progress: surface failures, follow the spoken word.
    pub fn poll_narration(&mut self) {
        if !self.narration.poll() {
            return;
        }
        if let Some(message) = self.narration.take_error() {
            self.set_status(message);
        }
        if let Some(word) = self.narration.current() {
            self.follow_word(word);
        }
    }

    fn follow_word(&mut self, spoken: usize) {
        let Some(source) = self.narration.source_index(spoken) else {
            return;
        };
        let Some(segment) = self
            .rendered
            .words
            .get(source)
            .and_then(|mark| mark.segments.first())
        else {
            return;
        };
        let height = self.viewport_height();
        let line = segment.line;
        if line < self.scroll || line >= self.scroll + height {
            self.scroll_to(line.saturating_sub(height / 4));
        }
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status.as_ref().and_then(|(message, at)| {
            if at.elapsed() < Duration::from_secs(4) {
                Some(message.as_str())
            } else {
                None
            }
        })
    }

    fn setup_watcher(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let (tx, rx): (Sender<()>, Receiver<()>) = channel();
        let result = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok() {
                let _ = tx.send(());
            }
        });
        match result {
            Ok(mut watcher) => {
                if watcher.watch(&path, RecursiveMode::NonRecursive).is_ok() {
                    self.watcher = Some(watcher);
                    self.watch_rx = Some(rx);
                }
            }
            Err(_) => {
                self.watch = false;
            }
        }
    }

    pub fn poll_watcher(&mut self) {
        if !self.watch {
            return;
        }
        let changed = self
            .watch_rx
            .as_ref()
            .is_some_and(|rx| rx.try_recv().is_ok());
        if changed {
            self.reload();
        }
    }

    pub fn toggle_watch(&mut self) {
        self.watch = !self.watch;
        if self.watch {
            self.setup_watcher();
            self.set_status("watch: on".into());
        } else {
            self.watcher = None;
            self.watch_rx = None;
            self.set_status("watch: off".into());
        }
    }

    pub fn open_link(&mut self, index: usize) {
        let Some(link) = self.rendered.links.get(index).cloned() else {
            return;
        };
        let url = link.url.clone();
        if url.starts_with('#') {
            let anchor = url.trim_start_matches('#').to_lowercase();
            let target =
                self.rendered.toc.iter().find(|entry| {
                    slug(&entry.title) == anchor || entry.title.to_lowercase() == anchor
                });
            match target {
                Some(entry) => {
                    let line = entry.line;
                    self.scroll_to(line);
                    self.set_status(format!("jumped to #{}", anchor));
                }
                None => self.set_status(format!("no heading matches {url}")),
            }
            return;
        }
        if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
            match open::that_detached(&url) {
                Ok(()) => self.set_status(format!("opened {url}")),
                Err(err) => self.set_status(format!("could not open link: {err}")),
            }
            return;
        }
        // Relative path: markdown files are opened in-place.
        let decoded = url.split('#').next().unwrap_or(&url);
        let path = PathBuf::from(decoded);
        let path = if path.is_absolute() {
            path
        } else {
            self.base_dir.join(path)
        };
        match self.load_path(&path) {
            Ok(()) => {}
            Err(err) => self.set_status(format!("could not open {decoded}: {err}")),
        }
    }

    pub fn next_link(&mut self, backwards: bool) {
        let count = self.rendered.links.len();
        if count == 0 {
            return;
        }
        let next = match self.link_selected {
            None => {
                if backwards {
                    count - 1
                } else {
                    0
                }
            }
            Some(current) => {
                if backwards {
                    (current + count - 1) % count
                } else {
                    (current + 1) % count
                }
            }
        };
        self.link_selected = Some(next);
        let line = self.rendered.links[next].line;
        if line < self.scroll || line >= self.scroll + self.viewport_height() {
            self.scroll_to(line.saturating_sub(self.viewport_height() / 3));
        }
    }

    pub fn jump_to_search_match(&mut self, backwards: bool) {
        if self.search.matches.is_empty() {
            return;
        }
        let count = self.search.matches.len();
        self.search.current = if backwards {
            (self.search.current + count - 1) % count
        } else {
            (self.search.current + 1) % count
        };
        let line = self.search.matches[self.search.current];
        self.scroll_to(line.saturating_sub(self.viewport_height() / 3));
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if let Some(path) = std::env::var_os("LEAFREAD_KEYLOG") {
            use std::io::Write as _;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(f, "{:?} {:?}", key.code, key.modifiers);
            }
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.files.is_some() {
            self.handle_files_key(key);
            return;
        }
        match self.mode {
            Mode::Search => self.handle_search_key(key),
            Mode::Toc => self.handle_toc_key(key),
            Mode::Links => self.handle_links_key(key),
            Mode::Help => self.mode = Mode::Normal,
            Mode::Normal => self.handle_normal_key(key),
        }
    }

    fn handle_files_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if self.path.is_some() {
                    self.files = None;
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(files) = &mut self.files {
                    files.move_down();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(files) = &mut self.files {
                    files.move_up();
                }
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                if let Some(files) = &mut self.files {
                    files.page(10);
                }
            }
            KeyCode::PageUp => {
                if let Some(files) = &mut self.files {
                    files.page(-10);
                }
            }
            KeyCode::Char('h') | KeyCode::Backspace => {
                if let Some(files) = &mut self.files {
                    files.parent();
                }
            }
            KeyCode::Enter => {
                let current = self.files.as_ref().and_then(|f| f.current()).cloned();
                if let Some(entry) = current {
                    if entry.is_dir {
                        if let Some(files) = &mut self.files {
                            files.enter_dir(&entry.path);
                        }
                    } else if let Err(err) = self.load_path(&entry.path) {
                        self.set_status(format!("could not open: {err}"));
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search.query = self.search.saved.clone();
                self.recompute_matches();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                if !self.search.matches.is_empty() {
                    self.jump_to_search_match(false);
                    self.set_status(format!(
                        "{}/{} matches",
                        self.search.current + 1,
                        self.search.matches.len()
                    ));
                } else {
                    self.set_status("no matches".into());
                }
            }
            KeyCode::Backspace => {
                self.search.query.pop();
                self.recompute_matches();
            }
            KeyCode::Char(ch) => {
                self.search.query.push(ch);
                self.recompute_matches();
                if let Some(line) = self.search.matches.first() {
                    self.scroll_to((*line).saturating_sub(self.viewport_height() / 3));
                }
            }
            _ => {}
        }
    }

    fn handle_toc_key(&mut self, key: KeyEvent) {
        let count = self.rendered.toc.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('t') => self.mode = Mode::Normal,
            KeyCode::Char('j') | KeyCode::Down if count > 0 => {
                self.toc_selected = (self.toc_selected + 1) % count;
            }
            KeyCode::Char('k') | KeyCode::Up if count > 0 => {
                self.toc_selected = (self.toc_selected + count - 1) % count;
            }
            KeyCode::Enter => {
                if let Some(entry) = self.rendered.toc.get(self.toc_selected) {
                    let line = entry.line;
                    self.jump_to_toc_line(line);
                }
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    pub fn jump_to_toc_line(&mut self, line: usize) {
        self.scroll_to(line.saturating_sub(1));
    }

    fn handle_links_key(&mut self, key: KeyEvent) {
        let count = self.rendered.links.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('l') => self.mode = Mode::Normal,
            KeyCode::Char('j') | KeyCode::Down if count > 0 => {
                self.link_selected = Some(match self.link_selected {
                    None => 0,
                    Some(i) => (i + 1) % count,
                });
            }
            KeyCode::Char('k') | KeyCode::Up if count > 0 => {
                self.link_selected = Some(match self.link_selected {
                    None => count - 1,
                    Some(i) => (i + count - 1) % count,
                });
            }
            KeyCode::Enter => {
                let selected = self.link_selected;
                if let Some(index) = selected {
                    self.mode = Mode::Normal;
                    self.open_link(index);
                }
            }
            _ => {}
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if self.narration.active() {
                    self.stop_narration();
                } else if !self.search.query.is_empty() {
                    self.search.query.clear();
                    self.recompute_matches();
                    self.set_status("search cleared".into());
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.scroll_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_by(-1),
            KeyCode::Char('d') => self.scroll_half_page(true),
            KeyCode::Char('u') => self.scroll_half_page(false),
            KeyCode::Char(' ') | KeyCode::PageDown => self.scroll_page(true),
            KeyCode::Char('b') | KeyCode::PageUp => self.scroll_page(false),
            KeyCode::Char('g') | KeyCode::Home => self.scroll_to(0),
            KeyCode::Char('G') | KeyCode::End => {
                let max = self.max_scroll();
                self.scroll_to(max);
            }
            KeyCode::Char('/') => {
                self.search.saved = self.search.query.clone();
                self.mode = Mode::Search;
            }
            KeyCode::Char('n') => self.jump_to_search_match(false),
            KeyCode::Char('N') => self.jump_to_search_match(true),
            KeyCode::Char('t') => {
                self.mode = Mode::Toc;
                self.toc_selected = self
                    .rendered
                    .toc
                    .iter()
                    .position(|entry| entry.line >= self.scroll)
                    .unwrap_or(0);
            }
            KeyCode::Char('l') => self.mode = Mode::Links,
            KeyCode::Tab => self.next_link(false),
            KeyCode::BackTab => self.next_link(true),
            KeyCode::Enter => {
                if let Some(index) = self.link_selected {
                    self.open_link(index);
                }
            }
            KeyCode::Char('o') => {
                if let Some(index) = self.link_selected {
                    self.open_link(index);
                } else {
                    self.set_status("tab to a link first, or press l".into());
                }
            }
            KeyCode::Char('f') => {
                let dir = self
                    .path
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_path_buf))
                    .unwrap_or_else(|| self.base_dir.clone());
                self.files = Some(FileBrowser::new(&dir));
            }
            KeyCode::Char('w') => self.toggle_watch(),
            KeyCode::Char('r') => self.reload(),
            KeyCode::Char('m') => {
                self.show_front_matter = !self.show_front_matter;
                self.relayout();
            }
            KeyCode::Char('p') => self.toggle_narration(),
            KeyCode::Char('s') => self.stop_narration(),
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
    }
}

pub fn line_text(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>()
}

pub fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            last_dash = false;
        } else if (ch == ' ' || ch == '-' || ch == '_') && !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_headings() {
        assert_eq!(slug("Hello, World!"), "hello-world");
        assert_eq!(slug("API: v2 — Notes"), "api-v2-notes");
        assert_eq!(slug("leading and trailing"), "leading-and-trailing");
    }

    #[test]
    fn app_renders_document_and_finds_toc_and_matches() {
        let initial = Initial {
            path: None,
            raw: "# Title\n\nhello world\n".into(),
            directory: None,
        };
        let mut app = App::new(
            initial,
            Theme::mono(),
            None,
            false,
            narration::Config::default(),
        );
        app.width = 60;
        app.height = 20;
        app.relayout();
        assert!(!app.rendered.lines.is_empty());
        assert_eq!(app.rendered.toc[0].title, "Title");
        app.search.query = "hello".into();
        app.recompute_matches();
        assert_eq!(app.search.matches.len(), 1);
        assert_eq!(app.max_scroll(), 0);
    }

    #[test]
    fn app_starts_in_directory_browser_mode() {
        let dir = std::env::temp_dir().join(format!("leafread-app-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("doc.md"), "# Doc\n").unwrap();
        let initial = Initial {
            path: None,
            raw: String::new(),
            directory: Some(dir.clone()),
        };
        let app = App::new(
            initial,
            Theme::mono(),
            None,
            false,
            narration::Config::default(),
        );
        assert!(app.files.is_some());
        let files = app.files.as_ref().unwrap();
        assert!(files.entries.iter().any(|e| e.name == "doc.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reading_aloud_with_nothing_to_speak_reports_it() {
        let initial = Initial {
            path: None,
            raw: String::new(),
            directory: None,
        };
        let mut app = App::new(
            initial,
            Theme::mono(),
            None,
            false,
            narration::Config::default(),
        );
        app.relayout();
        app.toggle_narration();
        assert!(!app.narration.active());
        assert_eq!(app.status_message(), Some("nothing to read aloud here"));
    }
}
