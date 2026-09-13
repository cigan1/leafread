//! comrak AST -> [`Document`] conversion.

use comrak::nodes::{AlertType, ListType, NodeValue, TableAlignment};
use comrak::{Arena, Options, parse_document};

use super::model::*;

/// Parse CommonMark + GFM (plus a few harmless extras) into the renderer model.
pub fn parse(source: &str) -> Document {
    let arena = Arena::new();
    let options = options();
    let root = parse_document(&arena, source, &options);

    let mut doc = Document::default();
    for node in root.children() {
        let value = node.data.borrow().value.clone();
        if let NodeValue::FrontMatter(text) = value {
            doc.front_matter = Some(text);
            continue;
        }
        convert_block(node, &mut doc.blocks);
    }
    doc
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.superscript = true;
    options.extension.footnotes = true;
    options.extension.description_lists = true;
    options.extension.front_matter_delimiter = Some("---".to_string());
    options.extension.multiline_block_quotes = true;
    options.extension.alerts = true;
    options.extension.math_dollars = true;
    options.extension.math_code = true;
    options.extension.underline = true;
    options.extension.subscript = true;
    options.extension.spoiler = true;
    options.extension.highlight = true;
    options.extension.shortcodes = true;
    options.extension.cjk_friendly_emphasis = true;
    options.parse.smart = true;
    options.parse.relaxed_tasklist_matching = true;
    options
}

/// Convert one block node. Unknown containers are descended into so that no
/// content is silently dropped.
fn convert_block<'a>(node: &'a comrak::nodes::AstNode<'a>, out: &mut Vec<Block>) {
    let value = node.data.borrow().value.clone();
    match value {
        NodeValue::Paragraph => {
            let inlines = convert_inlines(node);
            if let Some(tex) = standalone_display_math(&inlines) {
                out.push(Block::MathBlock(tex));
            } else if let Some((alt, url)) = standalone_image(&inlines) {
                out.push(Block::Image { url, alt });
            } else {
                out.push(Block::Paragraph(inlines));
            }
        }
        NodeValue::Heading(heading) => out.push(Block::Heading {
            level: heading.level,
            inlines: convert_inlines(node),
        }),
        NodeValue::CodeBlock(code) => {
            let info = code.info.trim().to_string();
            let lang = info.split_whitespace().next().map(str::to_string);
            let kind = match lang.as_deref() {
                Some("mermaid") => CodeKind::Mermaid,
                Some("math") | Some("latex") | Some("katex") => CodeKind::Math,
                _ => CodeKind::Code,
            };
            out.push(Block::CodeBlock {
                lang: lang.filter(|l| !l.is_empty()),
                code: code.literal,
                kind,
            });
        }
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
            let mut blocks = Vec::new();
            convert_children(node, &mut blocks);
            out.push(Block::Quote(blocks));
        }
        NodeValue::Alert(alert) => {
            let mut blocks = Vec::new();
            convert_children(node, &mut blocks);
            out.push(Block::Alert {
                kind: match alert.alert_type {
                    AlertType::Note => AlertKind::Note,
                    AlertType::Tip => AlertKind::Tip,
                    AlertType::Important => AlertKind::Important,
                    AlertType::Warning => AlertKind::Warning,
                    AlertType::Caution => AlertKind::Caution,
                },
                title: alert.title.clone(),
                blocks,
            });
        }
        NodeValue::List(list) => {
            let mut items = Vec::new();
            for item in node.children() {
                let item_value = item.data.borrow().value.clone();
                let mut blocks = Vec::new();
                let mut checked = None;
                if let NodeValue::TaskItem(task) = item_value {
                    checked = Some(task.symbol.is_some());
                }
                convert_children(item, &mut blocks);
                if let Some(Block::Paragraph(inlines)) = blocks.first_mut() {
                    if let Some(Inline::TaskMarker(done)) = inlines.first() {
                        checked = checked.or(Some(*done));
                        inlines.remove(0);
                    }
                    if checked.is_some() {
                        strip_leading_space(inlines);
                    }
                }
                items.push(ListItem { checked, blocks });
            }
            out.push(Block::List {
                ordered: list.list_type == ListType::Ordered,
                start: list.start as u64,
                tight: list.tight,
                items,
            });
        }
        NodeValue::Table(table) => {
            let mut header = Vec::new();
            let mut rows = Vec::new();
            for (row_index, row) in node.children().enumerate() {
                let mut cells = Vec::new();
                for cell in row.children() {
                    cells.push(convert_inlines(cell));
                }
                if row_index == 0 {
                    header = cells;
                } else {
                    rows.push(cells);
                }
            }
            let columns = table.num_columns.max(header.len());
            let aligns = (0..columns)
                .map(|i| {
                    table
                        .alignments
                        .get(i)
                        .copied()
                        .map(convert_align)
                        .unwrap_or_default()
                })
                .collect();
            out.push(Block::Table {
                aligns,
                header,
                rows,
            });
        }
        NodeValue::DescriptionList => {
            let mut items = Vec::new();
            for item in node.children() {
                let mut term = Vec::new();
                let mut details_blocks = Vec::new();
                for child in item.children() {
                    let child_value = child.data.borrow().value.clone();
                    match child_value {
                        NodeValue::DescriptionTerm => term = convert_inlines(child),
                        NodeValue::DescriptionDetails => {
                            convert_children(child, &mut details_blocks)
                        }
                        _ => {}
                    }
                }
                items.push((term, details_blocks));
            }
            out.push(Block::DescriptionList { items });
        }
        NodeValue::ThematicBreak => out.push(Block::Rule),
        NodeValue::HtmlBlock(html) => out.push(Block::Html(html.literal)),
        NodeValue::Math(math) if math.display_math => out.push(Block::MathBlock(math.literal)),
        NodeValue::FootnoteDefinition(def) => {
            let mut blocks = Vec::new();
            convert_children(node, &mut blocks);
            out.push(Block::FootnoteDef {
                name: def.name,
                blocks,
            });
        }
        NodeValue::Item(_) => {
            // A list item outside a list (only possible through odd parser paths).
            convert_children(node, out);
        }
        _ => {
            // Inline nodes reached at block level, or unknown containers: descend.
            convert_children(node, out);
        }
    }
}

fn convert_children<'a>(node: &'a comrak::nodes::AstNode<'a>, out: &mut Vec<Block>) {
    for child in node.children() {
        convert_block(child, out);
    }
}

fn convert_align(align: TableAlignment) -> Align {
    match align {
        TableAlignment::Left => Align::Left,
        TableAlignment::Center => Align::Center,
        TableAlignment::Right => Align::Right,
        TableAlignment::None => Align::None,
    }
}

/// A paragraph that contains only an image becomes a block-level image so the
/// viewer can reserve space for it.
fn standalone_image(inlines: &[Inline]) -> Option<(String, String)> {
    let mut image = None;
    for inline in inlines {
        match inline {
            Inline::Image { alt, url } => {
                if image.is_some() {
                    return None;
                }
                image = Some((alt.clone(), url.clone()));
            }
            Inline::Text(text) if text.trim().is_empty() => {}
            Inline::SoftBreak | Inline::LineBreak => {}
            _ => return None,
        }
    }
    image
}

/// A paragraph that contains only display math becomes a math block.
fn standalone_display_math(inlines: &[Inline]) -> Option<String> {
    let mut math = None;
    for inline in inlines {
        match inline {
            Inline::Math { tex, display: true } => {
                if math.is_some() {
                    return None;
                }
                math = Some(tex.clone());
            }
            Inline::Text(text) if text.trim().is_empty() => {}
            Inline::SoftBreak | Inline::LineBreak => {}
            _ => return None,
        }
    }
    math
}

fn strip_leading_space(inlines: &mut Vec<Inline>) {
    while let Some(Inline::Text(text)) = inlines.first_mut() {
        let trimmed = text.trim_start().to_string();
        if trimmed.is_empty() {
            inlines.remove(0);
        } else {
            *text = trimmed;
            break;
        }
    }
}

fn convert_inlines<'a>(node: &'a comrak::nodes::AstNode<'a>) -> Vec<Inline> {
    let mut out = Vec::new();
    for child in node.children() {
        let value = child.data.borrow().value.clone();
        match value {
            NodeValue::Text(text) => out.push(Inline::Text(text.to_string())),
            NodeValue::Code(code) => out.push(Inline::Code(code.literal)),
            NodeValue::SoftBreak => out.push(Inline::SoftBreak),
            NodeValue::LineBreak => out.push(Inline::LineBreak),
            NodeValue::Emph => out.push(Inline::Emph(convert_inlines(child))),
            NodeValue::Strong => out.push(Inline::Strong(convert_inlines(child))),
            NodeValue::Strikethrough => out.push(Inline::Strike(convert_inlines(child))),
            NodeValue::Underline => out.push(Inline::Underline(convert_inlines(child))),
            NodeValue::SpoileredText => out.push(Inline::Spoiler(convert_inlines(child))),
            NodeValue::Highlight => out.push(Inline::Emph(convert_inlines(child))),
            NodeValue::Superscript => out.push(Inline::Superscript(convert_inlines(child))),
            NodeValue::Subscript => out.push(Inline::Subscript(convert_inlines(child))),
            NodeValue::Link(link) => out.push(Inline::Link {
                text: convert_inlines(child),
                url: link.url,
            }),
            NodeValue::WikiLink(link) => out.push(Inline::Link {
                text: convert_inlines(child),
                url: link.url,
            }),
            NodeValue::Image(link) => out.push(Inline::Image {
                alt: plain_text(&convert_inlines(child)),
                url: link.url,
            }),
            NodeValue::Math(math) => out.push(Inline::Math {
                tex: math.literal,
                display: math.display_math,
            }),
            NodeValue::FootnoteReference(reference) => out.push(Inline::FootnoteRef {
                name: reference.name,
                index: reference.ix as usize,
            }),
            NodeValue::TaskItem(task) => out.push(Inline::TaskMarker(task.symbol.is_some())),
            NodeValue::HtmlInline(html) => out.push(Inline::Html(html)),
            NodeValue::EscapedTag(_) => {}
            _ => out.extend(convert_inlines(child)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_and_paragraphs() {
        let doc = parse("# Title\n\nHello **world**.\n");
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(
            doc.blocks[0],
            Block::Heading {
                level: 1,
                inlines: vec![Inline::Text("Title".into())]
            }
        );
        let Block::Paragraph(inlines) = &doc.blocks[1] else {
            panic!("expected paragraph");
        };
        assert_eq!(inlines.len(), 3);
        assert_eq!(
            inlines[1],
            Inline::Strong(vec![Inline::Text("world".into())])
        );
    }

    #[test]
    fn parses_gfm_table() {
        let doc = parse("| a | b |\n|:--|--:|\n| 1 | 2 |\n");
        let Block::Table {
            aligns,
            header,
            rows,
        } = &doc.blocks[0]
        else {
            panic!("expected table, got {:?}", doc.blocks[0]);
        };
        assert_eq!(aligns, &[Align::Left, Align::Right]);
        assert_eq!(header.len(), 2);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn parses_task_list() {
        let doc = parse("- [x] done\n- [ ] todo\n");
        let Block::List { items, .. } = &doc.blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(items[0].checked, Some(true));
        assert_eq!(items[1].checked, Some(false));
    }

    #[test]
    fn parses_front_matter() {
        let doc = parse("---\ntitle: Hello\n---\n# Hi\n");
        assert!(doc.front_matter.is_some());
        assert!(matches!(doc.blocks[0], Block::Heading { .. }));
    }

    #[test]
    fn parses_math_and_mermaid_fences() {
        let doc = parse("$$x^2$$\n\n```mermaid\ngraph TD\n```\n\n```math\nz\n```\n");
        assert_eq!(doc.blocks[0], Block::MathBlock("x^2".into()));
        assert!(matches!(
            &doc.blocks[1],
            Block::CodeBlock {
                kind: CodeKind::Mermaid,
                ..
            }
        ));
        assert!(matches!(
            &doc.blocks[2],
            Block::CodeBlock {
                kind: CodeKind::Math,
                ..
            }
        ));
    }

    #[test]
    fn parses_footnotes() {
        let doc = parse("Text[^1]\n\n[^1]: Note\n");
        let Block::Paragraph(inlines) = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        assert!(matches!(
            inlines.last(),
            Some(Inline::FootnoteRef { index: 1, .. })
        ));
        assert!(matches!(doc.blocks[1], Block::FootnoteDef { .. }));
    }

    #[test]
    fn parses_strikethrough_and_task_markers() {
        let doc = parse("~~gone~~ and `code`\n");
        let Block::Paragraph(inlines) = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(
            inlines[0],
            Inline::Strike(vec![Inline::Text("gone".into())])
        );
        assert!(inlines.includes_code());
    }

    trait IncludesCode {
        fn includes_code(&self) -> bool;
    }
    impl IncludesCode for Vec<Inline> {
        fn includes_code(&self) -> bool {
            self.iter().any(|i| matches!(i, Inline::Code(_)))
        }
    }
}
