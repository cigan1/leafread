//! Intermediate document model produced from the comrak AST and consumed by the
//! layout engine. Keeping the renderer decoupled from comrak means the AST can
//! change without touching layout code, and it makes the model easy to unit test.

/// Inline content within a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Underline(Vec<Inline>),
    Spoiler(Vec<Inline>),
    Superscript(Vec<Inline>),
    Subscript(Vec<Inline>),
    Link { text: Vec<Inline>, url: String },
    Image { alt: String, url: String },
    Math { tex: String, display: bool },
    FootnoteRef { name: String, index: usize },
    TaskMarker(bool),
    SoftBreak,
    LineBreak,
    Html(String),
}

/// Column alignment for a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    None,
    Left,
    Center,
    Right,
}

/// A list item, optionally a task-list checkbox item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

/// GitHub-style alert kinds (`> [!NOTE]` and friends).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    pub fn label(self) -> &'static str {
        match self {
            AlertKind::Note => "Note",
            AlertKind::Tip => "Tip",
            AlertKind::Important => "Important",
            AlertKind::Warning => "Warning",
            AlertKind::Caution => "Caution",
        }
    }
}

/// A block-level element.
// `CodeBlock`/`MathBlock` read better than the lint's suggested names.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    CodeBlock {
        lang: Option<String>,
        code: String,
        kind: CodeKind,
    },
    Quote(Vec<Block>),
    Alert {
        kind: AlertKind,
        title: Option<String>,
        blocks: Vec<Block>,
    },
    List {
        ordered: bool,
        start: u64,
        tight: bool,
        items: Vec<ListItem>,
    },
    Table {
        aligns: Vec<Align>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    DescriptionList {
        items: Vec<(Vec<Inline>, Vec<Block>)>,
    },
    Rule,
    Image {
        url: String,
        alt: String,
    },
    MathBlock(String),
    Html(String),
    FootnoteDef {
        name: String,
        blocks: Vec<Block>,
    },
}

/// What a fenced code block actually contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeKind {
    Code,
    Mermaid,
    Math,
}

/// A parsed Markdown document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub front_matter: Option<String>,
}

/// Plain-text rendering of a run of inlines; useful for TOC titles and table
/// width calculations.
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) | Inline::Code(t) | Inline::Html(t) => out.push_str(t),
            Inline::Emph(c)
            | Inline::Strong(c)
            | Inline::Strike(c)
            | Inline::Underline(c)
            | Inline::Spoiler(c)
            | Inline::Superscript(c)
            | Inline::Subscript(c) => out.push_str(&plain_text(c)),
            Inline::Link { text, url } => {
                let label = plain_text(text);
                if label.is_empty() {
                    out.push_str(url);
                } else {
                    out.push_str(&label);
                }
            }
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Math { tex, .. } => out.push_str(&crate::math::render(tex)),
            Inline::FootnoteRef { index, .. } => {
                out.push_str(&format!("[{index}]"));
            }
            Inline::TaskMarker(checked) => {
                out.push_str(if *checked { "☑ " } else { "☐ " });
            }
            Inline::SoftBreak | Inline::LineBreak => out.push(' '),
        }
    }
    out
}
