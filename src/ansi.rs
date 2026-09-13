//! Serialize rendered lines to ANSI escape sequences for piped output.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

use crate::markdown::layout::LinkSpan;

#[derive(Debug, Clone, Copy, Default)]
pub struct AnsiOptions {
    pub color: bool,
    pub hyperlinks: bool,
}

/// Serialize all lines, appending `\n` after each. Hyperlinks are emitted as
/// OSC 8 sequences so links stay clickable without cluttering the text.
pub fn render(lines: &[Line<'_>], links: &[LinkSpan], options: AnsiOptions) -> String {
    let mut out = String::new();
    let mut links_by_line: Vec<Vec<&LinkSpan>> = vec![Vec::new(); lines.len()];
    for link in links {
        if let Some(slot) = links_by_line.get_mut(link.line) {
            slot.push(link);
        }
    }
    for links in &mut links_by_line {
        links.sort_by_key(|l| l.start);
    }

    let mut current_style = None;
    let mut open_link: Option<usize> = None;
    for (index, line) in lines.iter().enumerate() {
        let line_links = &links_by_line[index];
        let mut col = 0usize;
        for span in &line.spans {
            if options.color {
                let desired = if span.style == Style::default() {
                    None
                } else {
                    Some(span.style)
                };
                if desired != current_style {
                    match desired {
                        Some(style) => out.push_str(&escape_for(style)),
                        None => out.push_str("\x1b[0m"),
                    }
                    current_style = desired;
                }
            }
            for ch in span.content.chars() {
                let containing = if options.hyperlinks {
                    line_links
                        .iter()
                        .position(|link| col >= link.start && col < link.end)
                } else {
                    None
                };
                if containing != open_link {
                    if open_link.is_some() {
                        out.push_str("\x1b]8;;\x1b\\");
                    }
                    if let Some(link_index) = containing
                        && let Some(link) = line_links.get(link_index)
                    {
                        out.push_str(&format!("\x1b]8;;{}\x1b\\", link.url));
                    }
                    open_link = containing;
                }
                out.push(ch);
                col += UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
            }
        }
        if open_link.take().is_some() {
            out.push_str("\x1b]8;;\x1b\\");
        }
        if options.color && current_style.take().is_some() {
            out.push_str("\x1b[0m");
        }
        out.push('\n');
    }
    out
}

fn escape_for(style: Style) -> String {
    let mut parts: Vec<String> = vec!["0".to_string()];
    if let Some(fg) = style.fg {
        if let Some(code) = color_code(fg, false) {
            parts.push(code);
        }
    }
    if let Some(bg) = style.bg {
        if let Some(code) = color_code(bg, true) {
            parts.push(code);
        }
    }
    let modifiers = style.add_modifier;
    for (flag, code) in [
        (Modifier::BOLD, "1"),
        (Modifier::DIM, "2"),
        (Modifier::ITALIC, "3"),
        (Modifier::UNDERLINED, "4"),
        (Modifier::SLOW_BLINK, "5"),
        (Modifier::RAPID_BLINK, "6"),
        (Modifier::REVERSED, "7"),
        (Modifier::HIDDEN, "8"),
        (Modifier::CROSSED_OUT, "9"),
    ] {
        if modifiers.contains(flag) {
            parts.push(code.to_string());
        }
    }
    format!("\x1b[{}m", parts.join(";"))
}

fn color_code(color: Color, background: bool) -> Option<String> {
    let (base, bright) = if background { (40, 100) } else { (30, 90) };
    Some(match color {
        Color::Reset => (base + 9).to_string(),
        Color::Black => base.to_string(),
        Color::Red => (base + 1).to_string(),
        Color::Green => (base + 2).to_string(),
        Color::Yellow => (base + 3).to_string(),
        Color::Blue => (base + 4).to_string(),
        Color::Magenta => (base + 5).to_string(),
        Color::Cyan => (base + 6).to_string(),
        Color::Gray => (base + 7).to_string(),
        Color::DarkGray => bright.to_string(),
        Color::LightRed => (bright + 1).to_string(),
        Color::LightGreen => (bright + 2).to_string(),
        Color::LightYellow => (bright + 3).to_string(),
        Color::LightBlue => (bright + 4).to_string(),
        Color::LightMagenta => (bright + 5).to_string(),
        Color::LightCyan => (bright + 6).to_string(),
        Color::White => (bright + 7).to_string(),
        Color::Indexed(n) => {
            if background {
                format!("48;5;{n}")
            } else {
                format!("38;5;{n}")
            }
        }
        Color::Rgb(r, g, b) => {
            if background {
                format!("48;2;{r};{g};{b}")
            } else {
                format!("38;2;{r};{g};{b}")
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Span;

    fn line(spans: Vec<Span<'static>>) -> Line<'static> {
        Line::from(spans)
    }

    #[test]
    fn plain_output_without_color() {
        let lines = vec![line(vec![Span::raw("hello")])];
        let out = render(&lines, &[], AnsiOptions::default());
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn emits_color_escapes() {
        let lines = vec![line(vec![Span::styled(
            "hi",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )])];
        let out = render(
            &lines,
            &[],
            AnsiOptions {
                color: true,
                hyperlinks: false,
            },
        );
        assert_eq!(out, "\x1b[0;31;1mhi\x1b[0m\n");
    }

    #[test]
    fn rgb_and_indexed_colors() {
        assert_eq!(
            escape_for(Style::default().fg(Color::Rgb(1, 2, 3))),
            "\x1b[0;38;2;1;2;3m"
        );
        assert_eq!(
            escape_for(Style::default().bg(Color::Indexed(200))),
            "\x1b[0;48;5;200m"
        );
    }

    #[test]
    fn emits_osc8_hyperlinks() {
        let lines = vec![line(vec![
            Span::raw("click "),
            Span::raw("here"),
            Span::raw("!"),
        ])];
        let links = vec![LinkSpan {
            line: 0,
            start: 6,
            end: 10,
            url: "https://example.com".into(),
            text: "here".into(),
        }];
        let out = render(
            &lines,
            &links,
            AnsiOptions {
                color: false,
                hyperlinks: true,
            },
        );
        assert_eq!(
            out,
            "click \x1b]8;;https://example.com\x1b\\here\x1b]8;;\x1b\\!\n"
        );
    }

    #[test]
    fn wraps_wide_characters_for_link_offsets() {
        let lines = vec![line(vec![Span::raw("日 "), Span::raw("link")])];
        let links = vec![LinkSpan {
            line: 0,
            start: 3,
            end: 7,
            url: "https://example.com".into(),
            text: "link".into(),
        }];
        let out = render(
            &lines,
            &links,
            AnsiOptions {
                color: false,
                hyperlinks: true,
            },
        );
        assert_eq!(
            out,
            "日 \x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\\n"
        );
    }
}
