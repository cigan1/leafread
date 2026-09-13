//! Lightweight Mermaid renderer.
//!
//! Renders `flowchart`/`graph` and `sequenceDiagram` sources as Unicode
//! diagrams. Anything else returns `None` so the caller can fall back to
//! showing the source with an "unsupported" badge.

use std::collections::HashMap;

/// Which flavour of diagram a source contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Flowchart,
    Sequence,
    Unsupported,
}

pub fn kind(source: &str) -> Kind {
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let mut words = line.split_whitespace();
        match words.next() {
            Some("flowchart") | Some("graph") => return Kind::Flowchart,
            Some("sequenceDiagram") => return Kind::Sequence,
            _ => return Kind::Unsupported,
        }
    }
    Kind::Unsupported
}

/// Render a Mermaid diagram, or `None` if the diagram type is unsupported.
pub fn render(source: &str) -> Option<Vec<String>> {
    match kind(source) {
        Kind::Flowchart => Some(render_flowchart(source)),
        Kind::Sequence => Some(render_sequence(source)),
        Kind::Unsupported => None,
    }
}

// ---------------------------------------------------------------------------
// Canvas
// ---------------------------------------------------------------------------

struct Canvas {
    cells: Vec<Vec<char>>,
}

impl Canvas {
    fn new() -> Self {
        Self { cells: Vec::new() }
    }

    fn ensure(&mut self, x: usize, y: usize) {
        while self.cells.len() <= y {
            self.cells.push(Vec::new());
        }
        while self.cells[y].len() <= x {
            self.cells[y].push(' ');
        }
    }

    fn put(&mut self, x: usize, y: usize, ch: char) {
        self.ensure(x, y);
        let existing = self.cells[y][x];
        self.cells[y][x] = merge(existing, ch);
    }

    fn put_str(&mut self, x: usize, y: usize, text: &str) {
        for (offset, ch) in text.chars().enumerate() {
            self.ensure(x + offset, y);
            self.cells[y][x + offset] = ch;
        }
    }

    fn hline(&mut self, x1: usize, x2: usize, y: usize, ch: char) {
        for x in x1.min(x2)..=x1.max(x2) {
            self.put(x, y, ch);
        }
    }

    fn vline(&mut self, x: usize, y1: usize, y2: usize, ch: char) {
        for y in y1.min(y2)..=y1.max(y2) {
            self.put(x, y, ch);
        }
    }

    fn to_lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .cells
            .iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_string())
            .collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines
    }
}

/// Merge two box-drawing characters that land on the same cell.
fn merge(a: char, b: char) -> char {
    if a == ' ' || a == b {
        return b;
    }
    if b == ' ' {
        return a;
    }
    // Arrowheads always win.
    for arrow in ['▼', '▲', '▶', '◀', '✕', '◉'] {
        if a == arrow {
            return a;
        }
        if b == arrow {
            return b;
        }
    }
    let pair = if a < b { (a, b) } else { (b, a) };
    match pair {
        ('─', '│') | ('│', '─') => '┼',
        ('─', '┌') | ('┌', '─') => '┬',
        ('─', '┐') | ('┐', '─') => '┬',
        ('─', '└') | ('└', '─') => '┴',
        ('─', '┘') | ('┘', '─') => '┴',
        ('│', '┌') | ('┌', '│') => '├',
        ('│', '┐') | ('┐', '│') => '┤',
        ('│', '└') | ('└', '│') => '├',
        ('│', '┘') | ('┘', '│') => '┤',
        ('┬', '│') | ('│', '┬') => '┼',
        ('┴', '│') | ('│', '┴') => '┼',
        ('├', '─') | ('─', '├') => '├',
        ('┤', '─') | ('─', '┤') => '┤',
        ('┬', '─') | ('─', '┬') => '┬',
        ('┴', '─') | ('─', '┴') => '┴',
        ('╌', '│') | ('│', '╌') => '┼',
        ('╎', '─') | ('─', '╎') => '┼',
        ('╌', '╎') | ('╎', '╌') => '┼',
        _ => '┼',
    }
}

// ---------------------------------------------------------------------------
// Flowchart
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Rect,
    Rounded,
    Stadium,
    Cylinder,
    Circle,
    Rhombus,
    Hexagon,
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    label: String,
    shape: Shape,
}

#[derive(Debug, Clone)]
struct Edge {
    from: String,
    to: String,
    label: Option<String>,
    dashed: bool,
}

#[derive(Debug, Default)]
struct Flowchart {
    direction: Option<Direction>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

fn render_flowchart(source: &str) -> Vec<String> {
    let fc = parse_flowchart(source);
    let Direction::TopDown = fc.direction.unwrap_or(Direction::TopDown) else {
        return render_flowchart_horizontal(&fc);
    };
    render_flowchart_vertical(&fc, true)
}

fn render_flowchart_horizontal(fc: &Flowchart) -> Vec<String> {
    let direction = fc.direction.unwrap_or(Direction::LeftRight);
    if matches!(direction, Direction::LeftRight | Direction::RightLeft) {
        render_flowchart_lr(fc)
    } else {
        render_flowchart_vertical(fc, matches!(direction, Direction::TopDown))
    }
}

fn parse_flowchart(source: &str) -> Flowchart {
    let mut fc = Flowchart::default();
    let mut index: HashMap<String, usize> = HashMap::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if line.starts_with("subgraph") || line == "end" {
            continue;
        }
        if line.starts_with("classDef")
            || line.starts_with("class ")
            || line.starts_with("style ")
            || line.starts_with("linkStyle")
            || line.starts_with("click ")
            || line.starts_with("direction ")
        {
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("flowchart")
            .or(line.strip_prefix("graph"))
        {
            fc.direction = parse_direction(rest.trim());
            continue;
        }
        for statement in line.split(';') {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }
            parse_statement(statement, &mut fc, &mut index);
        }
    }
    fc
}

fn parse_direction(text: &str) -> Option<Direction> {
    Some(match text {
        "TD" | "TB" => Direction::TopDown,
        "BT" => Direction::BottomUp,
        "LR" => Direction::LeftRight,
        "RL" => Direction::RightLeft,
        _ => return None,
    })
}

fn parse_statement(statement: &str, fc: &mut Flowchart, index: &mut HashMap<String, usize>) {
    let mut pos = 0;
    let Some((node, next)) = parse_node(statement, pos) else {
        return;
    };
    add_node(node, fc, index);
    pos = next;
    loop {
        let rest = &statement[pos..];
        let trimmed_start = rest.len() - rest.trim_start().len();
        pos += trimmed_start;
        let Some((edge, next)) = parse_arrow(&statement[pos..]) else {
            // Trailing node without an arrow.
            if let Some((node, next)) = parse_node(statement, pos) {
                add_node(node, fc, index);
                pos = next;
                continue;
            }
            break;
        };
        let Some((target, target_end)) = parse_node(statement, pos + next) else {
            break;
        };
        let from_id = fc.nodes[fc.nodes.len() - 1].id.clone();
        let previous = previous_node_id(statement, pos);
        let from = previous.unwrap_or(from_id);
        let to = target.id.clone();
        add_node(target, fc, index);
        fc.edges.push(Edge {
            from,
            to,
            label: edge.label,
            dashed: edge.dashed,
        });
        pos = target_end;
    }
}

/// The id of the node immediately preceding an arrow.
fn previous_node_id(statement: &str, arrow_pos: usize) -> Option<String> {
    let before = statement[..arrow_pos].trim_end();
    let mut end = before.len();
    let bytes = before.as_bytes();
    while end > 0 {
        let ch = bytes[end - 1] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            end -= 1;
        } else {
            break;
        }
    }
    if end == before.len() {
        return None;
    }
    Some(before[end..].to_string())
}

struct ParsedArrow {
    label: Option<String>,
    dashed: bool,
}

fn parse_arrow(input: &str) -> Option<(ParsedArrow, usize)> {
    // Order matters: longest / label-bearing patterns first.
    for (pattern, dashed) in [
        ("-->|", false),
        ("---|", false),
        ("==>", false),
        ("===", false),
        ("-.->", true),
        ("-.-", true),
        ("-->", false),
        ("---", false),
        ("<-->", false),
        ("--o", false),
        ("--x", false),
        ("o--o", false),
        ("x--x", false),
        (".->", true),
    ] {
        if let Some(rest) = input.strip_prefix(pattern) {
            if pattern.ends_with('|') {
                if let Some(end) = rest.find('|') {
                    let label = rest[..end].trim().to_string();
                    let consumed = pattern.len() + end + 1;
                    // Trailing arrow after the label (e.g. `-->|x| `).
                    let trailing = input[consumed..].trim_start();
                    let extra = input[consumed..].len() - trailing.len();
                    if trailing.starts_with("-->") || trailing.starts_with("---") {
                        return Some((
                            ParsedArrow {
                                label: Some(label),
                                dashed,
                            },
                            consumed + extra + 3,
                        ));
                    }
                    return Some((
                        ParsedArrow {
                            label: Some(label),
                            dashed,
                        },
                        consumed,
                    ));
                }
                continue;
            }
            return Some((
                ParsedArrow {
                    label: None,
                    dashed,
                },
                pattern.len(),
            ));
        }
    }
    // `-- text -->` style labels.
    let rest = input.strip_prefix("--")?;
    if let Some(arrow_index) = rest.find("-->").or_else(|| rest.find("---")) {
        let label = rest[..arrow_index].trim();
        if !label.is_empty() {
            return Some((
                ParsedArrow {
                    label: Some(label.to_string()),
                    dashed: false,
                },
                2 + arrow_index + 3,
            ));
        }
    }
    Some((
        ParsedArrow {
            label: None,
            dashed: false,
        },
        2,
    ))
}

fn parse_node(input: &str, mut pos: usize) -> Option<(Node, usize)> {
    let bytes = input.as_bytes();
    while pos < bytes.len() && bytes[pos] == b' ' {
        pos += 1;
    }
    let id_start = pos;
    while pos < bytes.len() {
        let ch = bytes[pos] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            pos += 1;
        } else {
            break;
        }
    }
    if pos == id_start {
        return None;
    }
    let id = input[id_start..pos].to_string();
    let (label, shape, end) = parse_shape(input, pos)?;
    Some((
        Node {
            id,
            label: label.unwrap_or_default(),
            shape,
        },
        end,
    ))
}

fn parse_shape(input: &str, pos: usize) -> Option<(Option<String>, Shape, usize)> {
    let rest = &input[pos..];
    let trimmed = rest.len() - rest.trim_start().len();
    let rest = rest.trim_start();
    let base = pos + trimmed;
    let (open, close, shape): (&str, &str, Shape) = if rest.starts_with("((") {
        ("((", "))", Shape::Circle)
    } else if rest.starts_with("([") {
        ("([", "])", Shape::Stadium)
    } else if rest.starts_with("[[") {
        ("[[", "]]", Shape::Rect)
    } else if rest.starts_with("[(") {
        ("[(", ")]", Shape::Cylinder)
    } else if rest.starts_with("{{") {
        ("{{", "}}", Shape::Hexagon)
    } else if rest.starts_with("[") {
        ("[", "]", Shape::Rect)
    } else if rest.starts_with("(") {
        ("(", ")", Shape::Rounded)
    } else if rest.starts_with("{") {
        ("{", "}", Shape::Rhombus)
    } else if rest.starts_with(">") {
        (">", "]", Shape::Rect)
    } else {
        return Some((None, Shape::Rect, base));
    };
    let inner_start = base + open.len();
    let end = input[inner_start..].find(close)? + inner_start;
    let mut label = input[inner_start..end].trim().to_string();
    if label.starts_with('"') && label.ends_with('"') && label.len() >= 2 {
        label = label[1..label.len() - 1].to_string();
    }
    Some((Some(label), shape, end + close.len()))
}

fn add_node(node: Node, fc: &mut Flowchart, index: &mut HashMap<String, usize>) {
    match index.get(&node.id) {
        Some(&i) => {
            if fc.nodes[i].label.is_empty() && !node.label.is_empty() {
                fc.nodes[i].label = node.label.clone();
            }
            if node.shape != Shape::Rect {
                fc.nodes[i].shape = node.shape;
            }
        }
        None => {
            index.insert(node.id.clone(), fc.nodes.len());
            fc.nodes.push(node);
        }
    }
}

fn node_label(node: &Node) -> String {
    let label = if node.label.is_empty() {
        node.id.clone()
    } else {
        node.label.clone()
    };
    let label = match node.shape {
        Shape::Circle => format!("({label})"),
        Shape::Rhombus => format!("⟨{label}⟩"),
        Shape::Hexagon => format!("{{{label}}}"),
        _ => label,
    };
    truncate(&label, 28)
}

fn truncate(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut width = 0;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + w > max.saturating_sub(1) {
            out.push('…');
            break;
        }
        out.push(ch);
        width += w;
    }
    out
}

fn node_width(node: &Node) -> usize {
    unicode_width::UnicodeWidthStr::width(node_label(node).as_str()) + 4
}

fn ranks(fc: &Flowchart) -> HashMap<String, usize> {
    let mut rank: HashMap<String, usize> = fc.nodes.iter().map(|n| (n.id.clone(), 0)).collect();
    for _ in 0..=fc.nodes.len() {
        let mut changed = false;
        for edge in &fc.edges {
            let from = *rank.get(&edge.from).unwrap_or(&0);
            let entry = rank.entry(edge.to.clone()).or_insert(0);
            if *entry < from + 1 {
                *entry = from + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    rank
}

fn render_flowchart_vertical(fc: &Flowchart, top_down: bool) -> Vec<String> {
    if fc.nodes.is_empty() {
        return vec!["(empty diagram)".to_string()];
    }
    let rank = ranks(fc);
    let max_rank = rank.values().copied().max().unwrap_or(0);
    let mut by_rank: Vec<Vec<&Node>> = vec![Vec::new(); max_rank + 1];
    for node in &fc.nodes {
        by_rank[*rank.get(&node.id).unwrap_or(&0)].push(node);
    }

    let mut canvas = Canvas::new();
    // Center each rank horizontally.
    let rank_widths: Vec<usize> = by_rank
        .iter()
        .map(|nodes| nodes.iter().map(|n| node_width(n) + 4).sum::<usize>())
        .collect();
    let total_width = rank_widths.iter().copied().max().unwrap_or(0);
    let mut positions: HashMap<String, (usize, usize)> = HashMap::new();
    let mut y = 0;
    let rank_order: Vec<usize> = if top_down {
        (0..by_rank.len()).collect()
    } else {
        (0..by_rank.len()).rev().collect()
    };
    for rank_index in rank_order {
        let nodes = &by_rank[rank_index];
        let mut x = (total_width.saturating_sub(rank_widths[rank_index])) / 2;
        for node in nodes {
            let width = node_width(node);
            draw_node(&mut canvas, x, y, node);
            positions.insert(node.id.clone(), (x + width / 2, y));
            x += width + 4;
        }
        y += 5;
    }

    for edge in &fc.edges {
        let (Some(&(x1, y1)), Some(&(x2, y2))) =
            (positions.get(&edge.from), positions.get(&edge.to))
        else {
            continue;
        };
        let vch = if edge.dashed { '╎' } else { '│' };
        let hch = if edge.dashed { '╌' } else { '─' };
        let src_bottom = y1 + 2;
        let dst_top = y2;
        if x1 == x2 {
            let (start, end) = if top_down {
                (src_bottom + 1, dst_top)
            } else {
                (dst_top + 3, src_bottom)
            };
            if start < end {
                canvas.vline(x1, start, end - 1, vch);
            }
            let head = if top_down { end - 1 } else { start - 1 };
            canvas.put(x1, head, if top_down { '▼' } else { '▲' });
        } else {
            let lane = if top_down {
                dst_top - 1
            } else {
                src_bottom + 1
            };
            let (vy_start, vy_end) = if top_down {
                (src_bottom + 1, lane)
            } else {
                (lane, dst_top + 3)
            };
            if vy_start < vy_end {
                canvas.vline(x1, vy_start, vy_end - 1, vch);
            }
            canvas.put(
                x1,
                lane,
                if x2 > x1 {
                    if top_down { '└' } else { '┐' }
                } else if top_down {
                    '┘'
                } else {
                    '┌'
                },
            );
            let (hx1, hx2) = (x1.min(x2) + 1, x1.max(x2).saturating_sub(1));
            if hx1 <= hx2 {
                canvas.hline(hx1, hx2, lane, hch);
            }
            if top_down {
                canvas.put(x2, lane, '▼');
            } else {
                canvas.put(x2, lane, '▲');
            }
        }
        if let Some(label) = &edge.label {
            let mid = (x1 + x2) / 2;
            let width = unicode_width::UnicodeWidthStr::width(label.as_str());
            let lx = mid.saturating_sub(width / 2);
            let ly = if top_down {
                dst_top - 2
            } else {
                src_bottom + 2
            };
            canvas.put_str(lx, ly, label);
        }
    }
    canvas.to_lines()
}

fn draw_node(canvas: &mut Canvas, x: usize, y: usize, node: &Node) {
    let label = node_label(node);
    let width = node_width(node);
    let (tl, tr, bl, br) = match node.shape {
        Shape::Rounded | Shape::Stadium | Shape::Cylinder => ('╭', '╮', '╰', '╯'),
        _ => ('┌', '┐', '└', '┘'),
    };
    canvas.put(x, y, tl);
    canvas.put(x + width - 1, y, tr);
    canvas.hline(x + 1, x + width - 2, y, '─');
    canvas.put(x, y + 1, '│');
    canvas.put(x + width - 1, y + 1, '│');
    canvas.put_str(x + 2, y + 1, &label);
    canvas.put(x, y + 2, bl);
    canvas.put(x + width - 1, y + 2, br);
    canvas.hline(x + 1, x + width - 2, y + 2, '─');
}

fn render_flowchart_lr(fc: &Flowchart) -> Vec<String> {
    if fc.nodes.is_empty() {
        return vec!["(empty diagram)".to_string()];
    }
    let rank = ranks(fc);
    let max_rank = rank.values().copied().max().unwrap_or(0);
    let mut by_rank: Vec<Vec<&Node>> = vec![Vec::new(); max_rank + 1];
    for node in &fc.nodes {
        by_rank[*rank.get(&node.id).unwrap_or(&0)].push(node);
    }
    let reverse = fc.direction == Some(Direction::RightLeft);
    let rank_order: Vec<usize> = if reverse {
        (0..by_rank.len()).rev().collect()
    } else {
        (0..by_rank.len()).collect()
    };
    let mut canvas = Canvas::new();
    let mut positions: HashMap<String, (usize, usize)> = HashMap::new();
    let mut x = 0;
    for rank_index in rank_order {
        let nodes = &by_rank[rank_index];
        let mut y = 0;
        let mut width = 0;
        for node in nodes {
            draw_node(&mut canvas, x, y, node);
            positions.insert(node.id.clone(), (x, y + 1));
            width = width.max(node_width(node));
            y += 5;
        }
        x += width + 6;
    }
    for edge in &fc.edges {
        let (Some(&(x1, y1)), Some(&(x2, y2))) =
            (positions.get(&edge.from), positions.get(&edge.to))
        else {
            continue;
        };
        let src_w = node_width(fc.nodes.iter().find(|n| n.id == edge.from).unwrap());
        let dst_w = node_width(fc.nodes.iter().find(|n| n.id == edge.to).unwrap());
        // `x` in positions is the left edge of the box; `y` is the middle row.
        let src_left = x1;
        let src_right = x1 + src_w - 1;
        let dst_left = x2;
        let dst_right = x2 + dst_w - 1;
        let forward = !reverse;
        if y1 == y2 {
            if forward {
                if src_right < dst_left.saturating_sub(1) {
                    canvas.hline(src_right + 1, dst_left - 1, y1, '─');
                }
                canvas.put(dst_left.saturating_sub(1), y1, '▶');
            } else {
                if dst_right < src_left.saturating_sub(1) {
                    canvas.hline(dst_right + 1, src_left - 1, y1, '─');
                }
                canvas.put(dst_right + 1, y1, '◀');
            }
        } else if forward {
            let lane = dst_left.saturating_sub(2);
            canvas.hline(src_right + 1, lane, y1, '─');
            canvas.put(lane, y1, if y2 > y1 { '╮' } else { '╯' });
            if y1 < y2 {
                canvas.vline(lane, y1 + 1, y2 - 1, '│');
            } else {
                canvas.vline(lane, y2 + 1, y1 - 1, '│');
            }
            canvas.put(lane, y2, if y2 > y1 { '╰' } else { '╭' });
            canvas.put(dst_left.saturating_sub(1), y2, '▶');
        } else {
            let lane = dst_right + 3;
            canvas.hline(src_left.saturating_sub(1), lane, y1, '─');
            canvas.put(lane, y1, if y2 > y1 { '╭' } else { '╰' });
            if y1 < y2 {
                canvas.vline(lane, y1 + 1, y2 - 1, '│');
            } else {
                canvas.vline(lane, y2 + 1, y1 - 1, '│');
            }
            canvas.put(lane, y2, if y2 > y1 { '╮' } else { '╯' });
            canvas.hline(dst_right + 2, lane.saturating_sub(1), y2, '─');
            canvas.put(dst_right + 1, y2, '◀');
        }
        if let Some(label) = &edge.label {
            let mid = (src_left + src_right + dst_left + dst_right) / 4;
            let ly = y1.min(y2).saturating_sub(1);
            canvas.put_str(mid.saturating_sub(label.len() / 2), ly, label);
        }
    }
    canvas.to_lines()
}

// ---------------------------------------------------------------------------
// Sequence diagram
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrowKind {
    Solid,
    Dashed,
    Cross,
    Async,
}

#[derive(Debug, Clone)]
enum Item {
    Message {
        from: String,
        to: String,
        text: String,
        kind: ArrowKind,
    },
    Note {
        over: Vec<String>,
        right_of: Option<String>,
        text: String,
    },
    BlockStart {
        label: String,
    },
    BlockElse {
        label: String,
    },
    BlockEnd,
}

#[derive(Debug, Default)]
struct Sequence {
    participants: Vec<(String, String)>,
    items: Vec<Item>,
}

fn render_sequence(source: &str) -> Vec<String> {
    let seq = parse_sequence(source);
    if seq.participants.is_empty() {
        return vec!["(empty diagram)".to_string()];
    }
    // Column layout.
    let widths: Vec<usize> = seq
        .participants
        .iter()
        .map(|(_, name)| unicode_width::UnicodeWidthStr::width(name.as_str()) + 4)
        .collect();
    let mut centers = Vec::new();
    let mut offset = 2;
    for width in &widths {
        centers.push(offset + width / 2);
        offset += width + 2;
    }
    let lane_of = |id: &str| -> Option<usize> {
        seq.participants
            .iter()
            .position(|(pid, _)| pid == id)
            .map(|i| centers[i])
    };
    let first_center = centers[0];
    let last_center = *centers.last().unwrap();

    let mut canvas = Canvas::new();
    let mut y = 0;
    // Participant headers.
    for (i, (_, name)) in seq.participants.iter().enumerate() {
        let center = centers[i];
        let width = unicode_width::UnicodeWidthStr::width(name.as_str()) + 4;
        let x = center - width / 2;
        canvas.put(x, y, '╭');
        canvas.hline(x + 1, x + width - 2, y, '─');
        canvas.put(x + width - 1, y, '╮');
        canvas.put(x, y + 1, '│');
        canvas.put_str(x + 2, y + 1, name);
        canvas.put(x + width - 1, y + 1, '│');
        canvas.put(x, y + 2, '╰');
        canvas.hline(x + 1, x + width - 2, y + 2, '─');
        canvas.put(x + width - 1, y + 2, '╯');
        canvas.put(center, y + 2, '┬');
    }
    y += 3;

    let lifelines = |canvas: &mut Canvas, y: usize| {
        for center in &centers {
            canvas.put(*center, y, '│');
        }
    };

    for item in &seq.items {
        match item {
            Item::Message {
                from,
                to,
                text,
                kind,
            } => {
                let (Some(x1), Some(x2)) = (lane_of(from), lane_of(to)) else {
                    continue;
                };
                // Label row.
                lifelines(&mut canvas, y);
                let label_x = if x1 <= x2 { x1 + 2 } else { x2 + 2 };
                canvas.put_str(label_x, y, text);
                y += 1;
                lifelines(&mut canvas, y);
                let fill = if *kind == ArrowKind::Dashed {
                    '╌'
                } else {
                    '─'
                };
                if x1 == x2 {
                    // Self message loop.
                    canvas.put(x1, y, '├');
                    canvas.hline(x1 + 1, x1 + 3, y, fill);
                    canvas.put(x1 + 4, y, '╮');
                    y += 1;
                    lifelines(&mut canvas, y);
                    canvas.put(x1, y, '│');
                    canvas.put(x1 + 1, y, '◀');
                    canvas.hline(x1 + 2, x1 + 3, y, fill);
                    canvas.put(x1 + 4, y, '╯');
                } else if x1 < x2 {
                    canvas.hline(x1 + 1, x2 - 2, y, fill);
                    canvas.put(
                        x2 - 1,
                        y,
                        match kind {
                            ArrowKind::Cross => '✕',
                            ArrowKind::Async => '>',
                            _ => '▶',
                        },
                    );
                } else {
                    canvas.hline(x2 + 2, x1 - 1, y, fill);
                    canvas.put(
                        x2 + 1,
                        y,
                        match kind {
                            ArrowKind::Cross => '✕',
                            ArrowKind::Async => '<',
                            _ => '◀',
                        },
                    );
                }
                y += 2;
            }
            Item::Note {
                over,
                right_of,
                text,
            } => {
                lifelines(&mut canvas, y);
                let (start, end) = if let Some(id) = right_of {
                    let x = lane_of(id).unwrap_or(first_center);
                    (x + 2, x + 2)
                } else if over.len() >= 2 {
                    (
                        lane_of(&over[0]).unwrap_or(first_center),
                        lane_of(&over[over.len() - 1]).unwrap_or(last_center),
                    )
                } else {
                    let x = lane_of(over.first().map(String::as_str).unwrap_or(""))
                        .unwrap_or(first_center);
                    (x - 1, x + 1)
                };
                let label = format!("┃ {text} ┃");
                let width = unicode_width::UnicodeWidthStr::width(label.as_str());
                let x = (start + end).saturating_sub(width) / 2;
                canvas.put_str(x, y, &label);
                y += 2;
            }
            Item::BlockStart { label } => {
                lifelines(&mut canvas, y);
                let x1 = first_center - 2;
                let x2 = last_center + 2;
                canvas.put(x1, y, '┌');
                canvas.hline(x1 + 1, x2 - 1, y, '─');
                canvas.put(x2, y, '┐');
                let tag = format!(" {label} ");
                canvas.put_str(x1 + 2, y, &tag);
                y += 1;
            }
            Item::BlockElse { label } => {
                lifelines(&mut canvas, y);
                let x1 = first_center - 2;
                let x2 = last_center + 2;
                canvas.put(x1, y, '├');
                canvas.hline(x1 + 1, x2 - 1, y, '─');
                canvas.put(x2, y, '┤');
                let tag = format!(" {label} ");
                canvas.put_str(x1 + 2, y, &tag);
                y += 1;
            }
            Item::BlockEnd => {
                lifelines(&mut canvas, y);
                let x1 = first_center - 2;
                let x2 = last_center + 2;
                canvas.put(x1, y, '└');
                canvas.hline(x1 + 1, x2 - 1, y, '─');
                canvas.put(x2, y, '┘');
                y += 1;
            }
        }
    }
    // Final lifelines.
    lifelines(&mut canvas, y);
    canvas.to_lines()
}

fn parse_sequence(source: &str) -> Sequence {
    let mut seq = Sequence::default();
    let mut declared: HashMap<String, usize> = HashMap::new();
    let ensure = |seq: &mut Sequence,
                  declared: &mut HashMap<String, usize>,
                  id: &str,
                  display: Option<String>| {
        if let Some(name) = display {
            if let Some(&i) = declared.get(id) {
                seq.participants[i].1 = name;
            } else {
                declared.insert(id.to_string(), seq.participants.len());
                seq.participants.push((id.to_string(), name));
            }
        } else if !declared.contains_key(id) {
            declared.insert(id.to_string(), seq.participants.len());
            seq.participants.push((id.to_string(), id.to_string()));
        }
    };
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") || line == "sequenceDiagram" {
            continue;
        }
        if line.starts_with("autonumber")
            || line.starts_with("activate ")
            || line.starts_with("deactivate ")
            || line.starts_with("box")
        {
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("participant")
            .or(line.strip_prefix("actor"))
        {
            let rest = rest.trim();
            if let Some((id, name)) = rest.split_once(" as ") {
                ensure(
                    &mut seq,
                    &mut declared,
                    id.trim(),
                    Some(name.trim().to_string()),
                );
            } else {
                ensure(&mut seq, &mut declared, rest, None);
            }
            continue;
        }
        for keyword in ["loop", "alt", "opt", "par", "critical", "break", "rect"] {
            if let Some(rest) = line.strip_prefix(keyword) {
                seq.items.push(Item::BlockStart {
                    label: format!("{keyword} {}", rest.trim()).trim().to_string(),
                });
                continue;
            }
        }
        if line == "end" {
            seq.items.push(Item::BlockEnd);
            continue;
        }
        if let Some(rest) = line.strip_prefix("else") {
            seq.items.push(Item::BlockElse {
                label: format!("else {}", rest.trim()).trim().to_string(),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("Note") {
            let rest = rest.trim();
            if let Some((where_part, text)) = rest.split_once(':') {
                let where_part = where_part.trim();
                let text = text.trim().to_string();
                if let Some(id) = where_part.strip_prefix("right of") {
                    let id = id.trim().to_string();
                    ensure(&mut seq, &mut declared, &id, None);
                    seq.items.push(Item::Note {
                        over: Vec::new(),
                        right_of: Some(id),
                        text,
                    });
                } else if let Some(id) = where_part.strip_prefix("left of") {
                    let id = id.trim().to_string();
                    ensure(&mut seq, &mut declared, &id, None);
                    seq.items.push(Item::Note {
                        over: Vec::new(),
                        right_of: Some(id),
                        text,
                    });
                } else if let Some(ids) = where_part.strip_prefix("over") {
                    let ids: Vec<String> = ids
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    for id in &ids {
                        ensure(&mut seq, &mut declared, id, None);
                    }
                    seq.items.push(Item::Note {
                        over: ids,
                        right_of: None,
                        text,
                    });
                }
            }
            continue;
        }
        // Message: `A->>B: text`
        if let Some((head, text)) = line.split_once(':')
            && let Some((from, to, kind)) = parse_message_head(head.trim())
        {
            ensure(&mut seq, &mut declared, &from, None);
            ensure(&mut seq, &mut declared, &to, None);
            seq.items.push(Item::Message {
                from,
                to,
                text: text.trim().to_string(),
                kind,
            });
        }
    }
    seq
}

fn parse_message_head(head: &str) -> Option<(String, String, ArrowKind)> {
    for (token, kind) in [
        ("-->>", ArrowKind::Dashed),
        ("->>", ArrowKind::Solid),
        ("-->", ArrowKind::Dashed),
        ("->", ArrowKind::Solid),
        ("-x", ArrowKind::Cross),
        ("-)", ArrowKind::Async),
    ] {
        if let Some(index) = head.find(token) {
            let from = head[..index].trim().to_string();
            let to = head[index + token.len()..].trim().to_string();
            if !from.is_empty() && !to.is_empty() {
                return Some((from, to, kind));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_diagram_kind() {
        assert_eq!(kind("graph TD\n A --> B"), Kind::Flowchart);
        assert_eq!(kind("flowchart LR\n A --> B"), Kind::Flowchart);
        assert_eq!(kind("sequenceDiagram\n A->>B: hi"), Kind::Sequence);
        assert_eq!(kind("classDiagram\n A <|-- B"), Kind::Unsupported);
    }

    #[test]
    fn renders_simple_flowchart() {
        let lines = render("graph TD\n    A[Start] --> B[End]\n").unwrap();
        let text = lines.join("\n");
        assert!(text.contains("Start"), "{text}");
        assert!(text.contains("End"), "{text}");
        assert!(text.contains('▼'), "{text}");
        assert!(text.contains('─'), "{text}");
    }

    #[test]
    fn renders_flowchart_labels_and_chains() {
        let lines = render("graph TD\n A[One] -->|yes| B[Two] --> C[Three]\n").unwrap();
        let text = lines.join("\n");
        assert!(text.contains("One") && text.contains("Two") && text.contains("Three"));
        assert!(text.contains("yes"));
    }

    #[test]
    fn renders_left_right_flowchart() {
        let lines = render("flowchart LR\n A[Start] --> B[End]\n").unwrap();
        let text = lines.join("\n");
        assert!(text.contains('▶'), "{text}");
        assert!(text.contains("Start") && text.contains("End"));
    }

    #[test]
    fn renders_sequence_diagram() {
        let lines = render(
            "sequenceDiagram\n    participant A as Alice\n    participant B as Bob\n    A->>B: Hello\n    B-->>A: Hi\n",
        )
        .unwrap();
        let text = lines.join("\n");
        assert!(text.contains("Alice") && text.contains("Bob"));
        assert!(text.contains("Hello") && text.contains("Hi"));
        assert!(text.contains('▶') && text.contains('◀'));
    }

    #[test]
    fn renders_sequence_blocks_and_notes() {
        let lines = render(
            "sequenceDiagram\n    A->>B: go\n    loop every day\n        B->>A: ping\n    end\n    Note over A,B: done\n",
        )
        .unwrap();
        let text = lines.join("\n");
        assert!(text.contains("loop every day"));
        assert!(text.contains("done"));
    }

    #[test]
    fn unsupported_returns_none() {
        assert!(render("pie\n \"a\": 1").is_none());
    }
}
