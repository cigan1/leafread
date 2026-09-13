//! Syntax highlighting via syntect, mapped onto ratatui styles.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme as SynTheme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Option<SynTheme>,
    fallback: Style,
}

impl Highlighter {
    pub fn new(syntax_theme: Option<&str>, fallback: Style) -> Self {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let theme =
            syntax_theme.and_then(|name| ThemeSet::load_defaults().themes.get(name).cloned());
        Self {
            syntaxes,
            theme,
            fallback,
        }
    }

    /// A highlighter that emits plain, unstyled lines.
    pub fn plain(fallback: Style) -> Self {
        Self {
            syntaxes: SyntaxSet::new(),
            theme: None,
            fallback,
        }
    }

    /// Highlight a fenced code block. Unknown languages fall back to plain text.
    pub fn highlight(&self, code: &str, lang: Option<&str>) -> Vec<Line<'static>> {
        let Some(theme) = &self.theme else {
            return plain_lines(code, self.fallback);
        };
        let syntax = lang
            .and_then(|l| self.syntaxes.find_syntax_by_token(l))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut out = Vec::new();
        for line in LinesWithEndings::from(code) {
            let ranges = highlighter
                .highlight_line(line, &self.syntaxes)
                .unwrap_or_default();
            let spans: Vec<Span<'static>> = ranges
                .iter()
                .map(|(style, text)| {
                    Span::styled(text.trim_end_matches('\n').to_string(), map_style(*style))
                })
                .collect();
            out.push(Line::from(spans));
        }
        if out.is_empty() {
            out.push(Line::default());
        }
        out
    }
}

fn plain_lines(code: &str, fallback: Style) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = code
        .lines()
        .map(|line| Line::from(Span::styled(line.to_string(), fallback)))
        .collect();
    if out.is_empty() {
        out.push(Line::default());
    }
    out
}

fn map_style(style: syntect::highlighting::Style) -> Style {
    let mut out = Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if style.font_style.contains(FontStyle::BOLD) {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        out = out.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        out = out.add_modifier(Modifier::UNDERLINED);
    }
    out
}
