//! Visual theme for terminal rendering.

use ratatui::style::{Color, Modifier, Style};

/// All colors and text styles used to render a document, plus the chrome of
/// the interactive viewer.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub text: Style,
    pub dim: Style,
    pub heading: [Style; 6],
    pub heading_underline: bool,
    pub link: Style,
    pub inline_code: Style,
    pub code_background: Color,
    pub code_border: Style,
    pub quote_text: Style,
    pub quote_bar: Style,
    pub rule: Style,
    pub list_marker: Style,
    pub table_border: Style,
    pub table_header: Style,
    pub alert_label: [Style; 5],
    pub alert_border: [Style; 5],
    pub math: Style,
    pub mermaid: Style,
    pub footnote: Style,
    pub search_match: Style,
    pub search_current: Style,
    pub toc_selected: Style,
    pub header_bar: Style,
    pub status_bar: Style,
    pub help_bar: Style,
    pub syntax_theme: Option<&'static str>,
}

impl Theme {
    pub fn from_name(name: &str) -> Self {
        match name {
            "light" => Self::light(),
            "mono" | "none" | "plain" => Self::mono(),
            _ => Self::dark(),
        }
    }

    pub fn dark() -> Self {
        let rgb = Color::Rgb;
        let accent = rgb(122, 162, 247);
        Self {
            name: "dark".into(),
            text: Style::default(),
            dim: Style::default().fg(rgb(110, 118, 145)),
            heading: [
                Style::default()
                    .fg(rgb(122, 162, 247))
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                Style::default()
                    .fg(rgb(187, 154, 247))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(125, 207, 255))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(115, 218, 202))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(224, 175, 104))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(160, 170, 200))
                    .add_modifier(Modifier::BOLD),
            ],
            heading_underline: true,
            link: Style::default()
                .fg(rgb(115, 218, 202))
                .add_modifier(Modifier::UNDERLINED),
            inline_code: Style::default().fg(rgb(255, 158, 100)).bg(rgb(38, 42, 60)),
            code_background: rgb(26, 27, 38),
            code_border: Style::default().fg(rgb(70, 78, 100)),
            quote_text: Style::default().fg(rgb(140, 148, 175)),
            quote_bar: Style::default().fg(rgb(90, 100, 130)),
            rule: Style::default().fg(rgb(70, 78, 100)),
            list_marker: Style::default().fg(accent),
            table_border: Style::default().fg(rgb(80, 90, 120)),
            table_header: Style::default()
                .fg(rgb(122, 162, 247))
                .add_modifier(Modifier::BOLD),
            alert_label: [
                Style::default()
                    .fg(rgb(122, 162, 247))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(115, 218, 202))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(187, 154, 247))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(224, 175, 104))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(247, 118, 142))
                    .add_modifier(Modifier::BOLD),
            ],
            alert_border: [
                Style::default().fg(rgb(122, 162, 247)),
                Style::default().fg(rgb(115, 218, 202)),
                Style::default().fg(rgb(187, 154, 247)),
                Style::default().fg(rgb(224, 175, 104)),
                Style::default().fg(rgb(247, 118, 142)),
            ],
            math: Style::default()
                .fg(rgb(187, 154, 247))
                .add_modifier(Modifier::ITALIC),
            mermaid: Style::default().fg(rgb(125, 207, 255)),
            footnote: Style::default().fg(rgb(140, 148, 175)),
            search_match: Style::default().fg(rgb(26, 27, 38)).bg(rgb(224, 175, 104)),
            search_current: Style::default().fg(rgb(26, 27, 38)).bg(rgb(247, 118, 142)),
            toc_selected: Style::default()
                .fg(rgb(26, 27, 38))
                .bg(rgb(122, 162, 247))
                .add_modifier(Modifier::BOLD),
            header_bar: Style::default().fg(rgb(169, 177, 214)).bg(rgb(26, 27, 38)),
            status_bar: Style::default().fg(rgb(169, 177, 214)).bg(rgb(26, 27, 38)),
            help_bar: Style::default().fg(rgb(110, 118, 145)),
            syntax_theme: Some("base16-ocean.dark"),
        }
    }

    pub fn light() -> Self {
        let rgb = Color::Rgb;
        let accent = rgb(52, 84, 209);
        Self {
            name: "light".into(),
            text: Style::default(),
            dim: Style::default().fg(rgb(120, 120, 130)),
            heading: [
                Style::default()
                    .fg(rgb(52, 84, 209))
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                Style::default()
                    .fg(rgb(124, 58, 237))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(3, 105, 161))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(15, 118, 110))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(180, 83, 9))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(80, 80, 90))
                    .add_modifier(Modifier::BOLD),
            ],
            heading_underline: true,
            link: Style::default()
                .fg(rgb(15, 118, 110))
                .add_modifier(Modifier::UNDERLINED),
            inline_code: Style::default().fg(rgb(190, 60, 20)).bg(rgb(238, 238, 242)),
            code_background: rgb(243, 243, 247),
            code_border: Style::default().fg(rgb(190, 190, 200)),
            quote_text: Style::default().fg(rgb(90, 90, 100)),
            quote_bar: Style::default().fg(rgb(160, 160, 175)),
            rule: Style::default().fg(rgb(180, 180, 190)),
            list_marker: Style::default().fg(accent),
            table_border: Style::default().fg(rgb(150, 150, 165)),
            table_header: Style::default()
                .fg(rgb(52, 84, 209))
                .add_modifier(Modifier::BOLD),
            alert_label: [
                Style::default()
                    .fg(rgb(52, 84, 209))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(15, 118, 110))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(124, 58, 237))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(180, 83, 9))
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(rgb(190, 40, 70))
                    .add_modifier(Modifier::BOLD),
            ],
            alert_border: [
                Style::default().fg(rgb(52, 84, 209)),
                Style::default().fg(rgb(15, 118, 110)),
                Style::default().fg(rgb(124, 58, 237)),
                Style::default().fg(rgb(180, 83, 9)),
                Style::default().fg(rgb(190, 40, 70)),
            ],
            math: Style::default()
                .fg(rgb(124, 58, 237))
                .add_modifier(Modifier::ITALIC),
            mermaid: Style::default().fg(rgb(3, 105, 161)),
            footnote: Style::default().fg(rgb(90, 90, 100)),
            search_match: Style::default().fg(rgb(255, 255, 255)).bg(rgb(180, 83, 9)),
            search_current: Style::default().fg(rgb(255, 255, 255)).bg(rgb(190, 40, 70)),
            toc_selected: Style::default()
                .fg(rgb(255, 255, 255))
                .bg(rgb(52, 84, 209))
                .add_modifier(Modifier::BOLD),
            header_bar: Style::default().fg(rgb(40, 40, 50)).bg(rgb(232, 232, 238)),
            status_bar: Style::default().fg(rgb(40, 40, 50)).bg(rgb(232, 232, 238)),
            help_bar: Style::default().fg(rgb(120, 120, 130)),
            syntax_theme: Some("InspiredGitHub"),
        }
    }

    /// No color at all: modifiers only. Safe for `NO_COLOR` and dumb terminals.
    pub fn mono() -> Self {
        let b = Style::default().add_modifier(Modifier::BOLD);
        let i = Style::default().add_modifier(Modifier::ITALIC);
        let u = Style::default().add_modifier(Modifier::UNDERLINED);
        let bu = Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        let d = Style::default().add_modifier(Modifier::DIM);
        let inv = Style::default().add_modifier(Modifier::REVERSED);
        Self {
            name: "mono".into(),
            text: Style::default(),
            dim: d,
            heading: [bu, b, b, b, b, b],
            heading_underline: false,
            link: u,
            inline_code: d,
            code_background: Color::Reset,
            code_border: d,
            quote_text: i,
            quote_bar: d,
            rule: d,
            list_marker: b,
            table_border: d,
            table_header: b,
            alert_label: [b, b, b, b, b],
            alert_border: [d, d, d, d, d],
            math: i,
            mermaid: d,
            footnote: d,
            search_match: inv,
            search_current: inv,
            toc_selected: inv,
            header_bar: Style::default().add_modifier(Modifier::REVERSED),
            status_bar: Style::default().add_modifier(Modifier::REVERSED),
            help_bar: d,
            syntax_theme: None,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
