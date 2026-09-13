//! Rendering the viewer: header, content, status bar, and overlays.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui_image::StatefulImage;

use crate::markdown::layout::ImagePlacement;
use crate::tui::app::{App, Mode, line_text};

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let header = Rect { height: 1, ..area };
        let status = Rect {
            y: area.y + area.height.saturating_sub(1),
            height: 1,
            ..area
        };
        let content = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(2),
            ..area
        };
        self.draw_header(frame, header);
        self.draw_content(frame, content);
        self.draw_status(frame, status);
        match self.mode {
            Mode::Toc => self.draw_toc(frame, area),
            Mode::Links => self.draw_links(frame, area),
            Mode::Help => self.draw_help(frame, area),
            _ => {}
        }
        if self.files.is_some() {
            self.draw_files(frame, area);
        }
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        if area.width == 0 {
            return;
        }
        let name = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "stdin".to_string());
        let title = self
            .rendered
            .title
            .as_ref()
            .map(|t| format!(" · {t}"))
            .unwrap_or_default();
        let left = format!(" leafread  {name}{title} ");
        let percent = if self.max_scroll() == 0 {
            100
        } else {
            (self.scroll * 100) / self.max_scroll()
        };
        let watch = if self.watch { " ● watch" } else { "" };
        let right = format!(" {percent}%{watch} ");
        let left_width = unicode_width::UnicodeWidthStr::width(left.as_str());
        let right_width = unicode_width::UnicodeWidthStr::width(right.as_str());
        let pad = (area.width as usize).saturating_sub(left_width + right_width);
        let text = format!("{left}{}{right}", " ".repeat(pad));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(text, self.theme.header_bar))),
            area,
        );
    }

    fn draw_content(&mut self, frame: &mut Frame, area: Rect) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        if self.rendered.lines.is_empty() {
            let message = if self.files.is_some() {
                "Select a file to read"
            } else {
                "(empty document)"
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(message, self.theme.dim))),
                inset(area, 1, 0),
            );
            return;
        }
        let height = area.height as usize;
        let start = self.scroll.min(self.rendered.lines.len());
        let end = (start + height).min(self.rendered.lines.len());
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(height);
        for (offset, line) in self.rendered.lines[start..end].iter().enumerate() {
            let line_index = start + offset;
            let mut highlighted = self.highlight_line(line, line_index);
            highlighted.spans.insert(0, Span::raw(" "));
            lines.push(highlighted);
        }
        while lines.len() < height {
            lines.push(Line::default());
        }
        frame.render_widget(Paragraph::new(Text::from(lines)), area);

        if self.mode == Mode::Normal && self.files.is_none() {
            self.draw_images(frame, area);
        }
    }

    fn highlight_line(&self, line: &Line<'static>, line_index: usize) -> Line<'static> {
        let query = self.search.query.to_lowercase();
        let current_match_line = self.search.matches.get(self.search.current).copied();
        if query.is_empty() {
            let mut line = line.clone();
            if let Some(selected) = self.link_selected
                && let Some(link) = self.rendered.links.get(selected)
                && link.line == line_index
            {
                line = line.style(Style::default().add_modifier(Modifier::REVERSED));
            }
            return line;
        }
        let text = line_text(line);
        let lower = text.to_lowercase();
        let mut match_ranges: Vec<(usize, usize)> = Vec::new();
        let mut from = 0;
        while let Some(position) = lower[from..].find(&query) {
            let start = from + position;
            let end = start + query.len();
            match_ranges.push((start, end));
            from = end.max(start + 1);
        }
        let is_current = current_match_line == Some(line_index);
        let base_style = line.style;
        let mut flat: Vec<(char, Style)> = Vec::new();
        for span in &line.spans {
            let style = base_style.patch(span.style);
            for ch in span.content.chars() {
                flat.push((ch, style));
            }
        }
        let mut byte_offsets = Vec::with_capacity(flat.len() + 1);
        let mut byte = 0usize;
        for (ch, _) in &flat {
            byte_offsets.push(byte);
            byte += ch.len_utf8();
        }
        byte_offsets.push(byte);
        let highlight = if is_current {
            self.theme.search_current
        } else {
            self.theme.search_match
        };
        let mut spans: Vec<Span<'static>> = Vec::new();
        for (index, (ch, style)) in flat.iter().enumerate() {
            let start_byte = byte_offsets[index];
            let end_byte = byte_offsets[index + 1];
            let in_match = match_ranges
                .iter()
                .any(|(s, e)| start_byte >= *s && end_byte <= *e);
            let style = if in_match {
                style.patch(highlight)
            } else {
                *style
            };
            match spans.last_mut() {
                Some(last) if last.style == style => last.content.to_mut().push(*ch),
                _ => spans.push(Span::styled(ch.to_string(), style)),
            }
        }
        Line::from(spans)
    }

    fn draw_images(&mut self, frame: &mut Frame, area: Rect) {
        if !self.images.is_enabled() {
            return;
        }
        let base_dir = self.base_dir.clone();
        let placements: Vec<(usize, ImagePlacement)> =
            self.rendered.images.iter().cloned().enumerate().collect();
        for (index, placement) in placements {
            let line = placement.line;
            if line < self.scroll || line >= self.scroll + area.height as usize {
                continue;
            }
            let y = area.y + (line - self.scroll) as u16;
            let height = (placement.rows as u16).min(area.bottom().saturating_sub(y));
            if height == 0 || area.width == 0 {
                continue;
            }
            let rect = Rect {
                x: area.x + 1,
                y,
                width: area.width.saturating_sub(2),
                height,
            };
            match self.images.protocol(index, &placement, &base_dir) {
                Some(protocol) => {
                    frame.render_stateful_widget(StatefulImage::default(), rect, protocol);
                }
                None => {
                    let label = if placement.alt.trim().is_empty() {
                        format!("🖼 {}", placement.src)
                    } else {
                        format!("🖼 {} — {}", placement.alt, placement.src)
                    };
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(label, self.theme.dim)))
                            .wrap(Wrap { trim: true }),
                        rect,
                    );
                }
            }
        }
    }

    fn draw_status(&self, frame: &mut Frame, area: Rect) {
        if area.width == 0 {
            return;
        }
        let (left, style) = match self.mode {
            Mode::Search => (
                format!("/{}", self.search.query),
                self.theme.status_bar,
            ),
            _ => match self.status_message() {
                Some(message) => (message.to_string(), self.theme.status_bar),
                None => (
                    "q quit  / search  n/N next  t toc  l links  tab links  f files  w watch  ? help"
                        .to_string(),
                    self.theme.help_bar,
                ),
            },
        };
        let position = format!(
            " {}:{} ",
            (self.scroll + 1).min(self.rendered.lines.len().max(1)),
            self.rendered.lines.len()
        );
        let left_width = unicode_width::UnicodeWidthStr::width(left.as_str());
        let right_width = unicode_width::UnicodeWidthStr::width(position.as_str());
        let pad = (area.width as usize).saturating_sub(left_width + right_width);
        let text = format!(" {left}{}{position}", " ".repeat(pad.saturating_sub(1)));
        frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
    }

    fn draw_toc(&self, frame: &mut Frame, area: Rect) {
        if self.rendered.toc.is_empty() {
            self.overlay_message(frame, area, "No headings");
            return;
        }
        let rect = centered(area, 60, 60);
        frame.render_widget(Clear, rect);
        let inner_height = rect.height.saturating_sub(2) as usize;
        let offset = self
            .toc_selected
            .saturating_sub(inner_height.saturating_sub(1));
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (index, entry) in self.rendered.toc.iter().enumerate().skip(offset) {
            if lines.len() >= inner_height {
                break;
            }
            let indent = "  ".repeat(entry.level.saturating_sub(1) as usize);
            let label = format!("{indent}{}", entry.title);
            if index == self.toc_selected {
                lines.push(Line::from(Span::styled(
                    format!("▶ {label}"),
                    self.theme.toc_selected,
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("  {label}"),
                    self.theme.text,
                )));
            }
        }
        let block = Block::bordered().title(" Contents (enter to jump, esc to close) ");
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), rect);
    }

    fn draw_links(&self, frame: &mut Frame, area: Rect) {
        if self.rendered.links.is_empty() {
            self.overlay_message(frame, area, "No links in this document");
            return;
        }
        let rect = centered(area, 70, 60);
        frame.render_widget(Clear, rect);
        let inner_height = rect.height.saturating_sub(2) as usize;
        let selected = self.link_selected.unwrap_or(0);
        let offset = selected.saturating_sub(inner_height.saturating_sub(1));
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (index, link) in self.rendered.links.iter().enumerate().skip(offset) {
            if lines.len() >= inner_height {
                break;
            }
            let label = format!("{}  →  {}", link.text, link.url);
            if index == selected {
                lines.push(Line::from(Span::styled(
                    format!("▶ {label}"),
                    self.theme.toc_selected,
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("  {label}"),
                    self.theme.text,
                )));
            }
        }
        let block = Block::bordered().title(" Links (enter to open, esc to close) ");
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), rect);
    }

    fn draw_help(&self, frame: &mut Frame, area: Rect) {
        let rect = centered(area, 60, 70);
        frame.render_widget(Clear, rect);
        let rows = [
            ("j / ↓", "scroll down"),
            ("k / ↑", "scroll up"),
            ("d / u", "half page down / up"),
            ("space / b", "page down / up"),
            ("g / G", "top / bottom"),
            ("/ then enter", "search"),
            ("n / N", "next / previous match"),
            ("t", "table of contents"),
            ("l", "link list"),
            ("tab / shift-tab", "select next / previous link"),
            ("enter / o", "open selected link"),
            ("f", "file browser"),
            ("w", "toggle live reload"),
            ("r", "reload file"),
            ("m", "toggle front matter"),
            ("q", "quit"),
        ];
        let mut lines = Vec::new();
        for (key, description) in rows {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key:<16}"), self.theme.link),
                Span::styled(description.to_string(), self.theme.text),
            ]));
        }
        let block = Block::bordered().title(format!(
            " Help — {} theme (any key to close) ",
            self.theme.name
        ));
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), rect);
    }

    fn draw_files(&self, frame: &mut Frame, area: Rect) {
        let Some(files) = &self.files else {
            return;
        };
        let rect = centered(area, 60, 70);
        frame.render_widget(Clear, rect);
        let inner_height = rect.height.saturating_sub(2) as usize;
        let offset = files
            .selected
            .saturating_sub(inner_height.saturating_sub(1));
        let mut lines: Vec<Line<'static>> = Vec::new();
        if files.entries.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (no markdown files here)",
                self.theme.dim,
            )));
        }
        for (index, entry) in files.entries.iter().enumerate().skip(offset) {
            if lines.len() >= inner_height {
                break;
            }
            let icon = if entry.is_dir { "📁" } else { "  " };
            let label = format!("{icon} {}", entry.name);
            if index == files.selected {
                lines.push(Line::from(Span::styled(
                    format!("▶ {label}"),
                    self.theme.toc_selected,
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("  {label}"),
                    self.theme.text,
                )));
            }
        }
        let title = format!(
            " {} (enter to open, h to go up, esc to close) ",
            files.dir.display()
        );
        let block = Block::bordered().title(title);
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), rect);
    }

    fn overlay_message(&self, frame: &mut Frame, area: Rect, message: &str) {
        let rect = centered(area, 50, 20);
        frame.render_widget(Clear, rect);
        let block = Block::bordered();
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                message.to_string(),
                self.theme.dim,
            )))
            .block(block),
            rect,
        );
    }
}

fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let width = area.width * percent_x / 100;
    let height = area.height * percent_y / 100;
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width: width.max(20).min(area.width),
        height: height.max(5).min(area.height),
    }
}

fn inset(area: Rect, x: u16, y: u16) -> Rect {
    Rect {
        x: area.x + x,
        y: area.y + y,
        width: area.width.saturating_sub(x * 2),
        height: area.height.saturating_sub(y * 2),
    }
}
