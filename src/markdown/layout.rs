//! Block layout: turns a [`Document`] into styled, wrapped terminal lines.
//!
//! The layout pass owns word wrapping, indentation, table drawing, code block
//! framing, and metadata (TOC entries, link spans, image placements) that the
//! interactive viewer and the ANSI serializer consume.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::model::*;
use super::syntax::Highlighter;
use super::theme::Theme;
use crate::mermaid;

/// Rows of terminal reserved for an inline image in the TUI.
pub const DEFAULT_IMAGE_ROWS: usize = 12;

#[derive(Clone, Copy)]
pub struct LayoutOptions<'a> {
    pub width: usize,
    pub theme: &'a Theme,
    pub highlighter: &'a Highlighter,
    pub show_front_matter: bool,
    pub reserve_image_rows: bool,
}

#[derive(Debug, Clone)]
pub struct TocEntry {
    pub level: u8,
    pub title: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct LinkSpan {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub url: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct ImagePlacement {
    pub line: usize,
    pub rows: usize,
    pub src: String,
    pub alt: String,
}

#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub lines: Vec<Line<'static>>,
    pub toc: Vec<TocEntry>,
    pub links: Vec<LinkSpan>,
    pub images: Vec<ImagePlacement>,
    pub footnotes: Vec<(String, Vec<Block>)>,
    pub title: Option<String>,
}

/// Render a document to styled lines plus navigation metadata.
pub fn render(doc: &Document, options: &LayoutOptions<'_>) -> Rendered {
    let mut builder = Builder::new(options);
    if options.show_front_matter
        && let Some(front) = &doc.front_matter
    {
        builder.render_front_matter(front);
    }
    builder.render_blocks(&doc.blocks, 0);
    builder.finish()
}

pub fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum()
}

// ---------------------------------------------------------------------------
// Wrapper
// ---------------------------------------------------------------------------

/// Wraps styled text to a width, emitting metadata about links as it goes.
struct Wrapper {
    width: usize,
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    col: usize,
    first_prefix: Vec<Span<'static>>,
    cont_prefix: Vec<Span<'static>>,
    first_prefix_width: usize,
    cont_prefix_width: usize,
    pending_word: Vec<Span<'static>>,
    pending_style: Style,
    pending_space: bool,
    started: bool,
    link: Option<String>,
    links: Vec<LinkSpan>,
}

impl Wrapper {
    fn new(
        width: usize,
        first_prefix: Vec<Span<'static>>,
        cont_prefix: Vec<Span<'static>>,
    ) -> Self {
        let first_prefix_width = spans_width(&first_prefix);
        let cont_prefix_width = spans_width(&cont_prefix);
        Self {
            width: width.max(4),
            lines: Vec::new(),
            current: Vec::new(),
            col: 0,
            first_prefix,
            cont_prefix,
            first_prefix_width,
            cont_prefix_width,
            pending_word: Vec::new(),
            pending_style: Style::default(),
            pending_space: false,
            started: false,
            link: None,
            links: Vec::new(),
        }
    }

    fn begin_link(&mut self, url: &str) {
        self.flush_word();
        self.link = Some(url.to_string());
    }

    fn end_link(&mut self) {
        self.flush_word();
        self.link = None;
    }

    fn next_prefix_width(&self) -> usize {
        if self.lines.is_empty() {
            self.first_prefix_width
        } else {
            self.cont_prefix_width
        }
    }

    fn begin_line(&mut self) {
        if self.started {
            return;
        }
        let prefix = if self.lines.is_empty() {
            self.first_prefix.clone()
        } else {
            self.cont_prefix.clone()
        };
        let width = spans_width(&prefix);
        self.current.extend(prefix);
        self.col = width;
        self.started = true;
    }

    fn newline(&mut self) {
        let line = Line::from(std::mem::take(&mut self.current));
        self.lines.push(line);
        self.started = false;
        self.col = 0;
    }

    fn hard_break(&mut self) {
        self.flush_word();
        self.pending_space = false;
        self.newline();
    }

    fn push_text(&mut self, text: &str, style: Style) {
        self.push_piece(text, style);
    }

    fn push_piece(&mut self, text: &str, style: Style) {
        for ch in text.chars() {
            if ch == '\n' {
                self.flush_word();
                self.pending_space = false;
                self.newline();
                continue;
            }
            if ch.is_whitespace() {
                self.flush_word();
                self.pending_space = true;
                continue;
            }
            if self.pending_style != style {
                self.flush_word();
            }
            self.pending_style = style;
            match self.pending_word.last_mut() {
                Some(last) if last.style == style => last.content.to_mut().push(ch),
                _ => self.pending_word.push(Span::styled(ch.to_string(), style)),
            }
        }
    }

    fn flush_word(&mut self) {
        if self.pending_word.is_empty() {
            return;
        }
        let word = std::mem::take(&mut self.pending_word);
        let word_width = spans_width(&word);
        let style = self.pending_style;

        self.begin_line();
        let prefix_width = self.next_prefix_width();
        let has_content = self.col > prefix_width;
        let space_pending = self.pending_space && has_content;

        if space_pending && self.col + 1 + word_width > self.width && has_content {
            self.newline();
            self.begin_line();
        }
        let prefix_width = self.next_prefix_width();
        let has_content = self.col > prefix_width;
        if self.pending_space && has_content && self.col < self.width {
            self.emit(" ".to_string(), style);
        }
        self.pending_space = false;

        if self.col + word_width <= self.width {
            for span in &word {
                self.emit(span.content.to_string(), span.style);
            }
        } else {
            // Word longer than the remaining space: hard-split by character.
            for span in &word {
                for ch in span.content.chars() {
                    let w = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if self.col + w > self.width && self.col > self.next_prefix_width() {
                        self.newline();
                        self.begin_line();
                    }
                    self.emit(ch.to_string(), span.style);
                }
            }
        }
    }

    fn emit(&mut self, text: String, style: Style) {
        let width = UnicodeWidthStr::width(text.as_str());
        self.begin_line();
        if let Some(url) = &self.link {
            let line = self.lines.len();
            let start = self.col;
            let end = start + width;
            if let Some(last) = self.links.last_mut()
                && last.line == line
                && last.end == start
                && last.url == *url
            {
                last.end = end;
                last.text.push_str(&text);
            } else {
                self.links.push(LinkSpan {
                    line,
                    start,
                    end,
                    url: url.clone(),
                    text: text.clone(),
                });
            }
        }
        self.current.push(Span::styled(text, style));
        self.col += width;
    }

    fn finish(mut self) -> (Vec<Line<'static>>, Vec<LinkSpan>) {
        self.flush_word();
        if !self.current.is_empty() || self.lines.is_empty() {
            self.newline();
        }
        (self.lines, self.links)
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

struct Builder<'a> {
    width: usize,
    theme: &'a Theme,
    highlighter: &'a Highlighter,
    reserve_image_rows: bool,
    out: Vec<Line<'static>>,
    toc: Vec<TocEntry>,
    links: Vec<LinkSpan>,
    images: Vec<ImagePlacement>,
    footnotes: Vec<(String, Vec<Block>)>,
    title: Option<String>,
}

impl<'a> Builder<'a> {
    fn new(options: &LayoutOptions<'a>) -> Self {
        Self {
            width: options.width.max(8),
            theme: options.theme,
            highlighter: options.highlighter,
            reserve_image_rows: options.reserve_image_rows,
            out: Vec::new(),
            toc: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
            footnotes: Vec::new(),
            title: None,
        }
    }

    fn sub(&self, blocks: &[Block], width: usize, depth: usize) -> Rendered {
        let mut builder = Builder {
            width: width.max(4),
            theme: self.theme,
            highlighter: self.highlighter,
            reserve_image_rows: self.reserve_image_rows,
            out: Vec::new(),
            toc: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
            footnotes: Vec::new(),
            title: None,
        };
        builder.render_blocks(blocks, depth);
        builder.finish()
    }

    fn finish(mut self) -> Rendered {
        self.render_footnotes();
        let min_keep = self
            .images
            .iter()
            .map(|image| image.line + image.rows)
            .max()
            .unwrap_or(0);
        while self.out.len() > min_keep && self.out.last().is_some_and(|l| l.spans.is_empty()) {
            self.out.pop();
        }
        Rendered {
            lines: self.out,
            toc: self.toc,
            links: self.links,
            images: self.images,
            footnotes: self.footnotes,
            title: self.title,
        }
    }

    fn render_footnotes(&mut self) {
        if self.footnotes.is_empty() {
            return;
        }
        self.push_blank();
        let mut header = vec![Span::styled("── Footnotes ", self.theme.dim)];
        header.push(Span::styled(
            "─".repeat(self.width.saturating_sub(14)),
            self.theme.dim,
        ));
        self.out.push(Line::from(header));
        self.push_blank();
        let mut queue: std::collections::VecDeque<(String, Vec<Block>)> =
            std::mem::take(&mut self.footnotes).into();
        let mut number = 0usize;
        while let Some((_name, blocks)) = queue.pop_front() {
            number += 1;
            let marker = format!("[{number}] ");
            let marker_width = spans_width(&[Span::raw(marker.clone())]);
            let sub = self.sub(&blocks, self.width.saturating_sub(marker_width), 0);
            self.append_prefixed(
                sub,
                vec![Span::styled(marker, self.theme.footnote)],
                vec![Span::styled(" ".repeat(marker_width), self.theme.footnote)],
                None,
            );
            // Nested footnote definitions are queued behind the current one.
            queue.extend(std::mem::take(&mut self.footnotes));
            if !queue.is_empty() {
                self.push_blank();
            }
        }
    }

    fn push_lines(&mut self, lines: Vec<Line<'static>>, links: Vec<LinkSpan>) {
        let base = self.out.len();
        self.out.extend(lines);
        for mut link in links {
            link.line += base;
            self.links.push(link);
        }
    }

    fn push_blank(&mut self) {
        if self.out.last().is_none_or(|l| !l.spans.is_empty()) {
            self.out.push(Line::default());
        }
    }

    fn ensure_blank_before(&mut self) {
        if !self.out.is_empty() && self.out.last().is_some_and(|l| !l.spans.is_empty()) {
            self.out.push(Line::default());
        }
    }

    fn append_prefixed(
        &mut self,
        sub: Rendered,
        first_prefix: Vec<Span<'static>>,
        cont_prefix: Vec<Span<'static>>,
        line_style: Option<Style>,
    ) {
        let base = self.out.len();
        let first_width = spans_width(&first_prefix);
        let cont_width = spans_width(&cont_prefix);
        for (i, line) in sub.lines.into_iter().enumerate() {
            let prefix = if i == 0 {
                first_prefix.clone()
            } else {
                cont_prefix.clone()
            };
            let mut spans = prefix;
            let blank = line.spans.is_empty();
            spans.extend(line.spans);
            let mut new_line = Line::from(spans);
            if let Some(style) = line_style {
                new_line = new_line.style(style);
            }
            if blank {
                new_line = Line::default();
            }
            self.out.push(new_line);
        }
        for mut link in sub.links {
            link.line += base;
            let width = if link.line == base {
                first_width
            } else {
                cont_width
            };
            link.start += width;
            link.end += width;
            self.links.push(link);
        }
        for entry in sub.toc {
            self.toc.push(TocEntry {
                line: entry.line + base,
                ..entry
            });
        }
        for mut image in sub.images {
            image.line += base;
            self.images.push(image);
        }
        self.footnotes.extend(sub.footnotes);
    }

    fn render_front_matter(&mut self, front: &str) {
        let label = Span::styled("── front matter ", self.theme.dim);
        let mut line = Line::from(vec![label]);
        line.spans.push(Span::styled(
            "─".repeat(self.width.saturating_sub(16)),
            self.theme.dim,
        ));
        self.out.push(line);
        for raw in front.lines() {
            let text = raw.trim_end_matches(['\r', '\n']);
            let mut spans = vec![Span::styled("│ ", self.theme.quote_bar)];
            spans.push(Span::styled(text.to_string(), self.theme.dim));
            self.out.push(Line::from(spans));
        }
        self.push_blank();
    }

    fn render_blocks(&mut self, blocks: &[Block], depth: usize) {
        for block in blocks {
            self.render_block(block, depth);
        }
    }

    fn render_block(&mut self, block: &Block, depth: usize) {
        match block {
            Block::Heading { level, inlines } => self.render_heading(*level, inlines),
            Block::Paragraph(inlines) => self.render_paragraph(inlines),
            Block::CodeBlock { lang, code, kind } => match kind {
                CodeKind::Code => self.render_code(code, lang.as_deref()),
                CodeKind::Math => self.render_math_block(code),
                CodeKind::Mermaid => self.render_mermaid_block(code),
            },
            Block::Quote(blocks) => self.render_quote(blocks, depth),
            Block::Alert {
                kind,
                title,
                blocks,
            } => self.render_alert(*kind, title.as_deref(), blocks, depth),
            Block::List {
                ordered,
                start,
                tight,
                items,
            } => self.render_list(*ordered, *start, *tight, items, depth),
            Block::Table {
                aligns,
                header,
                rows,
            } => self.render_table(aligns, header, rows),
            Block::DescriptionList { items } => self.render_description_list(items, depth),
            Block::Rule => {
                self.ensure_blank_before();
                self.out.push(Line::from(Span::styled(
                    "─".repeat(self.width),
                    self.theme.rule,
                )));
                self.push_blank();
            }
            Block::Image { url, alt } => self.render_image(url, alt),
            Block::MathBlock(tex) => self.render_math_block(tex),
            Block::Html(html) => self.render_html(html),
            Block::FootnoteDef { name, blocks } => {
                self.footnotes.push((name.clone(), blocks.clone()));
            }
        }
    }

    fn render_heading(&mut self, level: u8, inlines: &[Inline]) {
        let level = level.clamp(1, 6) as usize;
        let title = plain_text(inlines);
        if level == 1 && self.title.is_none() && !title.trim().is_empty() {
            self.title = Some(title.trim().to_string());
        }
        self.ensure_blank_before();
        let line = self.out.len();
        self.toc.push(TocEntry {
            level: level as u8,
            title: title.trim().to_string(),
            line,
        });
        let style = self.theme.heading[level - 1];
        let mut wrapper = Wrapper::new(self.width, Vec::new(), Vec::new());
        push_inlines(&mut wrapper, inlines, style, self.theme);
        let (lines, links) = wrapper.finish();
        self.push_lines(lines, links);
        if level <= 2 && self.theme.heading_underline && self.width > 4 {
            let ch = if level == 1 { '━' } else { '─' };
            let len = self.width.min(if level == 1 { 60 } else { 48 });
            self.out.push(Line::from(Span::styled(
                ch.to_string().repeat(len),
                Style::default().fg(match self.theme.heading[level - 1].fg {
                    Some(color) => color,
                    None => ratatui::style::Color::Reset,
                }),
            )));
        }
        self.push_blank();
    }

    fn render_paragraph(&mut self, inlines: &[Inline]) {
        self.ensure_blank_before();
        let mut wrapper = Wrapper::new(self.width, Vec::new(), Vec::new());
        push_inlines(&mut wrapper, inlines, self.theme.text, self.theme);
        let (lines, links) = wrapper.finish();
        self.push_lines(lines, links);
        self.push_blank();
    }

    fn render_code(&mut self, code: &str, lang: Option<&str>) {
        let highlighted = self.highlighter.highlight(code, lang);
        if self.width < 10 {
            for line in highlighted {
                let mut spans = vec![Span::styled("  ", self.theme.text)];
                spans.extend(clip_spans(&line.spans, self.width.saturating_sub(2)));
                self.out.push(Line::from(spans));
            }
            self.push_blank();
            return;
        }
        self.ensure_blank_before();
        let border = self.theme.code_border;
        let bg = self.theme.code_background;
        let label = lang.unwrap_or("");
        let mut top = vec![Span::styled("╭─", border)];
        if !label.is_empty() {
            top.push(Span::styled(format!(" {label} "), border));
        }
        let used: usize = spans_width(&top);
        top.push(Span::styled(
            "─".repeat(self.width.saturating_sub(used + 1)),
            border,
        ));
        top.push(Span::styled("╮", border));
        self.out
            .push(Line::from(top).style(Style::default().bg(bg)));

        for line in highlighted {
            let mut spans = vec![Span::styled("│ ", border)];
            let content = clip_spans(&line.spans, self.width.saturating_sub(4));
            let content_width = spans_width(&content);
            spans.extend(content);
            spans.push(Span::styled(
                " ".repeat(self.width.saturating_sub(4 + content_width)),
                self.theme.text,
            ));
            spans.push(Span::styled(" │", border));
            self.out
                .push(Line::from(spans).style(Style::default().bg(bg)));
        }

        let mut bottom = vec![Span::styled("╰", border)];
        bottom.push(Span::styled(
            "─".repeat(self.width.saturating_sub(2)),
            border,
        ));
        bottom.push(Span::styled("╯", border));
        self.out
            .push(Line::from(bottom).style(Style::default().bg(bg)));
        self.push_blank();
    }

    fn render_math_block(&mut self, tex: &str) {
        self.ensure_blank_before();
        let rendered = crate::math::render(tex);
        for line in rendered.lines() {
            let text = line.trim();
            let width = UnicodeWidthStr::width(text);
            let pad = self.width.saturating_sub(width) / 2;
            let mut spans = vec![Span::styled(" ".repeat(pad), self.theme.text)];
            spans.push(Span::styled(text.to_string(), self.theme.math));
            self.out.push(Line::from(spans));
        }
        self.push_blank();
    }

    fn render_mermaid_block(&mut self, code: &str) {
        if let Some(diagram) = mermaid::render(code) {
            self.ensure_blank_before();
            for line in diagram {
                self.out
                    .push(Line::from(Span::styled(line, self.theme.mermaid)));
            }
            self.push_blank();
            return;
        }
        self.ensure_blank_before();
        let kind = match mermaid::kind(code) {
            mermaid::Kind::Unsupported => "unsupported diagram type",
            _ => "could not lay out diagram",
        };
        self.out.push(Line::from(Span::styled(
            format!("⚠ mermaid: {kind}"),
            self.theme.alert_label[3],
        )));
        let plain = Highlighter::plain(self.theme.text);
        let highlighted = plain.highlight(code, None);
        let bg = self.theme.code_background;
        for line in highlighted {
            let mut spans = vec![Span::styled("│ ", self.theme.code_border)];
            let content = clip_spans(&line.spans, self.width.saturating_sub(4));
            let content_width = spans_width(&content);
            spans.extend(content);
            spans.push(Span::styled(
                " ".repeat(self.width.saturating_sub(4 + content_width)),
                self.theme.text,
            ));
            spans.push(Span::styled(" │", self.theme.code_border));
            self.out
                .push(Line::from(spans).style(Style::default().bg(bg)));
        }
        self.push_blank();
    }

    fn render_quote(&mut self, blocks: &[Block], depth: usize) {
        self.ensure_blank_before();
        let inner_width = self.width.saturating_sub(2);
        let sub = self.sub(blocks, inner_width, depth + 1);
        let bar = vec![Span::styled("│ ", self.theme.quote_bar)];
        let count = sub.lines.len();
        self.append_prefixed(sub, bar.clone(), bar, Some(self.theme.quote_text));
        if count == 0 {
            self.out
                .push(Line::from(Span::styled("│", self.theme.quote_bar)));
        }
        self.push_blank();
    }

    fn render_alert(
        &mut self,
        kind: AlertKind,
        title: Option<&str>,
        blocks: &[Block],
        depth: usize,
    ) {
        self.ensure_blank_before();
        let index = match kind {
            AlertKind::Note => 0,
            AlertKind::Tip => 1,
            AlertKind::Important => 2,
            AlertKind::Warning => 3,
            AlertKind::Caution => 4,
        };
        let label = title.unwrap_or_else(|| kind.label());
        self.out.push(Line::from(vec![
            Span::styled("▌ ", self.theme.alert_border[index]),
            Span::styled(label.to_string(), self.theme.alert_label[index]),
        ]));
        let sub = self.sub(blocks, self.width.saturating_sub(2), depth + 1);
        let prefix = vec![Span::styled("▌ ", self.theme.alert_border[index])];
        self.append_prefixed(sub, prefix.clone(), prefix, None);
        self.push_blank();
    }

    fn render_list(
        &mut self,
        ordered: bool,
        start: u64,
        tight: bool,
        items: &[ListItem],
        depth: usize,
    ) {
        if items.is_empty() {
            return;
        }
        self.ensure_blank_before();
        let number_width = if ordered {
            (start + items.len() as u64 - 1).to_string().len()
        } else {
            0
        };
        for (i, item) in items.iter().enumerate() {
            let (marker, marker_style) = match item.checked {
                Some(checked) => (
                    if checked { "☑ " } else { "☐ " }.to_string(),
                    self.theme.list_marker,
                ),
                None if ordered => (
                    format!("{:>width$}. ", start + i as u64, width = number_width),
                    self.theme.list_marker,
                ),
                None => (format!("{} ", bullet_char(depth)), self.theme.list_marker),
            };
            let marker_width = UnicodeWidthStr::width(marker.as_str());
            let sub = self.sub(
                &item.blocks,
                self.width.saturating_sub(marker_width),
                depth + 1,
            );
            self.append_prefixed(
                sub,
                vec![Span::styled(marker, marker_style)],
                vec![Span::styled(" ".repeat(marker_width), self.theme.text)],
                None,
            );
            if !tight {
                self.push_blank();
            }
        }
        self.push_blank();
    }

    fn render_table(
        &mut self,
        aligns: &[Align],
        header: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
    ) {
        let columns = aligns
            .len()
            .max(header.len())
            .max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if columns == 0 {
            return;
        }
        self.ensure_blank_before();

        let normalize = |row: &[Vec<Inline>]| -> Vec<Vec<Inline>> {
            (0..columns)
                .map(|i| row.get(i).cloned().unwrap_or_default())
                .collect()
        };
        let header = normalize(header);
        let rows: Vec<Vec<Vec<Inline>>> = rows.iter().map(|r| normalize(r)).collect();

        let mut widths = vec![3usize; columns];
        for cell in std::iter::once(&header).chain(rows.iter()) {
            for (i, content) in cell.iter().enumerate() {
                let width = UnicodeWidthStr::width(plain_text(content).as_str());
                widths[i] = widths[i].max(width.min(48));
            }
        }
        let overhead = (columns + 1) + 2 * columns;
        let available = self.width.saturating_sub(overhead).max(columns);
        while widths.iter().sum::<usize>() > available {
            let Some((index, _)) = widths
                .iter()
                .enumerate()
                .filter(|(_, w)| **w > 3)
                .max_by_key(|(_, w)| **w)
            else {
                break;
            };
            widths[index] -= 1;
        }

        let mut lines: Vec<Line<'static>> = Vec::new();
        let border = self.theme.table_border;
        let horizontal = |left: char, mid: char, right: char| -> Line<'static> {
            let mut text = String::new();
            text.push(left);
            for (i, w) in widths.iter().enumerate() {
                text.push_str(&"─".repeat(w + 2));
                text.push(if i + 1 == widths.len() { right } else { mid });
            }
            Line::from(Span::styled(text, border))
        };

        lines.push(horizontal('┌', '┬', '┐'));
        lines.extend(self.table_row(&header, &widths, aligns, true));
        lines.push(horizontal('├', '┼', '┤'));
        for (i, row) in rows.iter().enumerate() {
            lines.extend(self.table_row(row, &widths, aligns, false));
            if i + 1 < rows.len() {
                lines.push(horizontal('├', '┼', '┤'));
            }
        }
        lines.push(horizontal('└', '┴', '┘'));
        self.out.extend(lines);
        self.push_blank();
    }

    fn table_row(
        &self,
        cells: &[Vec<Inline>],
        widths: &[usize],
        aligns: &[Align],
        is_header: bool,
    ) -> Vec<Line<'static>> {
        let mut wrapped: Vec<Vec<Line<'static>>> = Vec::new();
        let mut height = 1;
        for (i, cell) in cells.iter().enumerate() {
            let width = widths.get(i).copied().unwrap_or(3);
            let mut wrapper = Wrapper::new(width, Vec::new(), Vec::new());
            push_inlines(&mut wrapper, cell, self.theme.text, self.theme);
            let (mut lines, _) = wrapper.finish();
            if lines.is_empty() {
                lines.push(Line::default());
            }
            height = height.max(lines.len());
            wrapped.push(lines);
        }
        let mut out = Vec::new();
        for row_index in 0..height {
            let mut spans = vec![Span::styled("│", self.theme.table_border)];
            for (i, cell_lines) in wrapped.iter().enumerate() {
                let width = widths.get(i).copied().unwrap_or(3);
                let align = aligns.get(i).copied().unwrap_or_default();
                let content = cell_lines.get(row_index);
                let pad = width.saturating_sub(content.map_or(0, line_width));
                let (left, right) = match align {
                    Align::Right => (pad, 0),
                    Align::Center => (pad / 2, pad - pad / 2),
                    Align::Left | Align::None => (0, pad),
                };
                spans.push(Span::styled(" ", self.theme.text));
                if left > 0 {
                    spans.push(Span::styled(" ".repeat(left), self.theme.text));
                }
                if let Some(line) = content {
                    spans.extend(line.spans.iter().cloned());
                }
                if right > 0 {
                    spans.push(Span::styled(" ".repeat(right), self.theme.text));
                }
                spans.push(Span::styled(" ", self.theme.text));
                spans.push(Span::styled("│", self.theme.table_border));
            }
            let line = Line::from(spans);
            out.push(if is_header {
                line.style(self.theme.table_header)
            } else {
                line
            });
        }
        out
    }

    fn render_description_list(&mut self, items: &[(Vec<Inline>, Vec<Block>)], depth: usize) {
        for (term, details) in items {
            self.ensure_blank_before();
            let mut wrapper = Wrapper::new(self.width, Vec::new(), Vec::new());
            push_inlines(
                &mut wrapper,
                term,
                self.theme.text.add_modifier(Modifier::BOLD),
                self.theme,
            );
            let (lines, links) = wrapper.finish();
            self.push_lines(lines, links);
            let sub = self.sub(details, self.width.saturating_sub(2), depth + 1);
            let prefix = vec![Span::styled("  ", self.theme.text)];
            self.append_prefixed(sub, prefix.clone(), prefix, None);
        }
        self.push_blank();
    }

    fn render_image(&mut self, url: &str, alt: &str) {
        self.ensure_blank_before();
        let label = if alt.trim().is_empty() {
            url.to_string()
        } else {
            format!("{alt} — {url}")
        };
        if self.reserve_image_rows {
            let line = self.out.len();
            self.images.push(ImagePlacement {
                line,
                rows: DEFAULT_IMAGE_ROWS,
                src: url.to_string(),
                alt: alt.to_string(),
            });
            for _ in 0..DEFAULT_IMAGE_ROWS {
                self.out.push(Line::default());
            }
        } else {
            self.out.push(Line::from(Span::styled(
                format!("🖼 {label}"),
                self.theme.dim,
            )));
        }
        self.push_blank();
    }

    fn render_html(&mut self, html: &str) {
        self.ensure_blank_before();
        for raw in html.lines() {
            let text = strip_tags(raw);
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let mut wrapper = Wrapper::new(self.width, Vec::new(), Vec::new());
            wrapper.push_text(text, self.theme.dim);
            let (lines, links) = wrapper.finish();
            self.push_lines(lines, links);
        }
        self.push_blank();
    }
}

// ---------------------------------------------------------------------------
// Inline rendering
// ---------------------------------------------------------------------------

fn push_inlines(wrapper: &mut Wrapper, inlines: &[Inline], base: Style, theme: &Theme) {
    for inline in inlines {
        push_inline(wrapper, inline, base, theme);
    }
}

fn push_inline(wrapper: &mut Wrapper, inline: &Inline, base: Style, theme: &Theme) {
    match inline {
        Inline::Text(text) => wrapper.push_text(text, base),
        Inline::Code(code) => wrapper.push_text(code, base.patch(theme.inline_code)),
        Inline::Emph(children) => push_inlines(
            wrapper,
            children,
            base.add_modifier(Modifier::ITALIC),
            theme,
        ),
        Inline::Strong(children) => {
            push_inlines(wrapper, children, base.add_modifier(Modifier::BOLD), theme)
        }
        Inline::Strike(children) => push_inlines(
            wrapper,
            children,
            base.add_modifier(Modifier::CROSSED_OUT),
            theme,
        ),
        Inline::Underline(children) => push_inlines(
            wrapper,
            children,
            base.add_modifier(Modifier::UNDERLINED),
            theme,
        ),
        Inline::Spoiler(children) => {
            push_inlines(wrapper, children, base.add_modifier(Modifier::DIM), theme)
        }
        Inline::Superscript(children) => push_inlines(wrapper, children, base, theme),
        Inline::Subscript(children) => push_inlines(wrapper, children, base, theme),
        Inline::Link { text, url } => {
            wrapper.begin_link(url);
            push_inlines(wrapper, text, base.patch(theme.link), theme);
            wrapper.end_link();
        }
        Inline::Image { alt, url } => {
            wrapper.begin_link(url);
            let label = if alt.trim().is_empty() {
                url.clone()
            } else {
                alt.clone()
            };
            wrapper.push_text(&format!("🖼 {label}"), base.patch(theme.link));
            wrapper.end_link();
        }
        Inline::Math { tex, .. } => {
            wrapper.push_text(&crate::math::render(tex), base.patch(theme.math))
        }
        Inline::FootnoteRef { index, .. } => {
            wrapper.push_text(&format!("[{index}]"), base.patch(theme.footnote))
        }
        Inline::TaskMarker(checked) => wrapper.push_text(if *checked { "☑ " } else { "☐ " }, base),
        Inline::SoftBreak => wrapper.push_text(" ", base),
        Inline::LineBreak => wrapper.hard_break(),
        Inline::Html(html) => {
            let text = strip_tags(html);
            if !text.is_empty() {
                wrapper.push_text(&text, base.patch(theme.dim));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bullet_char(depth: usize) -> char {
    match depth % 3 {
        0 => '•',
        1 => '◦',
        _ => '▪',
    }
}

pub fn spans_width(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum()
}

/// Truncate spans to `max` display columns, adding an ellipsis when cut.
pub fn clip_spans(spans: &[Span<'static>], max: usize) -> Vec<Span<'static>> {
    if spans_width(spans) <= max {
        return spans.to_vec();
    }
    let budget = max.saturating_sub(1);
    let mut out = Vec::new();
    let mut used = 0;
    let mut last_style = Style::default();
    'outer: for span in spans {
        last_style = span.style;
        let mut text = String::new();
        for ch in span.content.chars() {
            if ch == '\t' {
                for _ in 0..4 {
                    if used + 1 > budget {
                        if !text.is_empty() {
                            out.push(Span::styled(text, span.style));
                        }
                        break 'outer;
                    }
                    text.push(' ');
                    used += 1;
                }
                continue;
            }
            let w = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
            if used + w > budget {
                if !text.is_empty() {
                    out.push(Span::styled(text, span.style));
                }
                break 'outer;
            }
            text.push(ch);
            used += w;
        }
        if !text.is_empty() {
            out.push(Span::styled(text, span.style));
        }
    }
    out.push(Span::styled("…", last_style));
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_text(source: &str, width: usize) -> Rendered {
        let theme = Theme::mono();
        let highlighter = Highlighter::plain(theme.text);
        let doc = crate::markdown::parser::parse(source);
        let options = LayoutOptions {
            width,
            theme: &theme,
            highlighter: &highlighter,
            show_front_matter: false,
            reserve_image_rows: false,
        };
        render(&doc, &options)
    }

    fn text(rendered: &Rendered) -> String {
        rendered
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn wraps_long_paragraphs() {
        let rendered = render_text(
            "This is a long paragraph that should wrap across several lines nicely.",
            20,
        );
        assert!(rendered.lines.len() > 1);
        for line in &rendered.lines {
            assert!(line_width(line) <= 20, "{:?}", text(&rendered));
        }
    }

    #[test]
    fn records_toc_entries() {
        let rendered = render_text("# One\n\n## Two\n\n### Three\n", 40);
        assert_eq!(rendered.toc.len(), 3);
        assert_eq!(rendered.toc[0].level, 1);
        assert_eq!(rendered.toc[0].title, "One");
        assert!(rendered.toc[1].line > rendered.toc[0].line);
    }

    #[test]
    fn records_link_spans() {
        let rendered = render_text("See [the docs](https://example.com) now.", 40);
        assert_eq!(rendered.links.len(), 1);
        assert_eq!(rendered.links[0].url, "https://example.com");
        assert!(rendered.links[0].text.contains("the docs"));
    }

    #[test]
    fn renders_task_lists() {
        let rendered = render_text("- [x] done\n- [ ] todo\n", 40);
        let output = text(&rendered);
        assert!(output.contains("☑ done"), "{output}");
        assert!(output.contains("☐ todo"), "{output}");
    }

    #[test]
    fn renders_nested_lists() {
        let rendered = render_text("- outer\n  - inner\n", 40);
        let output = text(&rendered);
        assert!(output.contains("• outer"), "{output}");
        assert!(output.contains("◦ inner"), "{output}");
    }

    #[test]
    fn renders_tables_with_borders() {
        let rendered = render_text("| a | b |\n|---|---|\n| 1 | 2 |\n", 40);
        let output = text(&rendered);
        assert!(output.contains('┌'), "{output}");
        assert!(output.contains("│ a"), "{output}");
        assert!(output.contains("│ 1"), "{output}");
        assert!(output.contains('└'), "{output}");
    }

    #[test]
    fn table_cells_wrap_within_width() {
        let source = "| header | header |\n|---|---|\n| a very long cell value here | short |\n";
        let rendered = render_text(source, 30);
        for line in &rendered.lines {
            assert!(line_width(line) <= 30, "{:?}", text(&rendered));
        }
    }

    #[test]
    fn renders_code_blocks_with_border() {
        let rendered = render_text("```rust\nfn main() {}\n```\n", 40);
        let output = text(&rendered);
        assert!(output.contains("╭─ rust"), "{output}");
        assert!(output.contains("│ fn main() {}"), "{output}");
        assert!(output.contains('╰'), "{output}");
    }

    #[test]
    fn clips_long_code_lines() {
        let rendered = render_text("```\n0123456789012345678901234567890123456789\n```\n", 20);
        for line in &rendered.lines {
            assert!(line_width(line) <= 20, "{:?}", text(&rendered));
        }
    }

    #[test]
    fn renders_blockquotes_with_bar() {
        let rendered = render_text("> quoted text\n", 40);
        let output = text(&rendered);
        assert!(output.contains("│ quoted text"), "{output}");
    }

    #[test]
    fn renders_mermaid_flowchart_without_source() {
        let rendered = render_text("```mermaid\ngraph TD\n A[Start] --> B[End]\n```\n", 40);
        let output = text(&rendered);
        assert!(output.contains("Start"), "{output}");
        assert!(!output.contains("graph TD"), "{output}");
    }

    #[test]
    fn renders_unsupported_mermaid_as_source_with_badge() {
        let rendered = render_text("```mermaid\npie\n \"a\": 1\n```\n", 40);
        let output = text(&rendered);
        assert!(output.contains("unsupported"), "{output}");
        assert!(output.contains("pie"), "{output}");
    }

    #[test]
    fn renders_math_blocks_centered() {
        let rendered = render_text("$$\nx^2 + y^2\n$$\n", 40);
        let output = text(&rendered);
        assert!(output.contains("x² + y²"), "{output}");
    }

    #[test]
    fn renders_footnotes_section() {
        let rendered = render_text("Text[^1]\n\n[^1]: The note\n", 40);
        let output = text(&rendered);
        assert!(output.contains("[1] The note"), "{output}");
    }

    #[test]
    fn long_words_are_split() {
        let rendered = render_text("supercalifragilisticexpialidocious", 10);
        for line in &rendered.lines {
            assert!(line_width(line) <= 10, "{:?}", text(&rendered));
        }
    }

    #[test]
    fn images_reserve_rows_in_tui_mode() {
        let theme = Theme::mono();
        let highlighter = Highlighter::plain(theme.text);
        let doc = crate::markdown::parser::parse("![alt](img.png)\n");
        let options = LayoutOptions {
            width: 40,
            theme: &theme,
            highlighter: &highlighter,
            show_front_matter: false,
            reserve_image_rows: true,
        };
        let rendered = render(&doc, &options);
        assert_eq!(rendered.images.len(), 1);
        assert_eq!(rendered.images[0].src, "img.png");
        assert!(rendered.lines.len() >= DEFAULT_IMAGE_ROWS);
    }

    #[test]
    fn alert_renders_label() {
        let rendered = render_text("> [!WARNING]\n> Careful!\n", 40);
        let output = text(&rendered);
        assert!(output.contains("Warning"), "{output}");
        assert!(output.contains("Careful!"), "{output}");
    }
}
