use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "drawer", about = "Render JSON Canvas (.canvas) files to SVG", version)]
struct Cli {
    /// Input .canvas file
    input: PathBuf,
    /// Output .svg file
    #[arg(short, long)]
    output: PathBuf,
    /// Padding around the canvas content
    #[arg(long, default_value_t = 40)]
    padding: i64,
}

#[derive(Debug, Deserialize)]
struct Canvas {
    #[serde(default)]
    nodes: Vec<Node>,
    #[serde(default)]
    edges: Vec<Edge>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Node {
    #[serde(rename = "text")]
    Text {
        id: String,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
        #[serde(default)]
        color: Option<String>,
        text: String,
    },
    #[serde(rename = "file")]
    File {
        id: String,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
        #[serde(default)]
        color: Option<String>,
        file: String,
    },
    #[serde(rename = "link")]
    Link {
        id: String,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
        #[serde(default)]
        color: Option<String>,
        url: String,
    },
    #[serde(rename = "group")]
    Group {
        id: String,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        label: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Edge {
    #[allow(dead_code)]
    id: String,
    from_node: String,
    to_node: String,
    #[serde(default)]
    from_side: Option<String>,
    #[serde(default)]
    to_side: Option<String>,
    #[serde(default)]
    from_end: Option<String>,
    #[serde(default)]
    to_end: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    label: Option<String>,
}

impl Node {
    fn id(&self) -> &str {
        match self {
            Node::Text { id, .. }
            | Node::File { id, .. }
            | Node::Link { id, .. }
            | Node::Group { id, .. } => id,
        }
    }

    fn bounds(&self) -> (i64, i64, i64, i64) {
        match self {
            Node::Text {
                x, y, width, height, ..
            }
            | Node::File {
                x, y, width, height, ..
            }
            | Node::Link {
                x, y, width, height, ..
            }
            | Node::Group {
                x, y, width, height, ..
            } => (*x, *y, *width, *height),
        }
    }

    fn color(&self) -> Option<&str> {
        match self {
            Node::Text { color, .. }
            | Node::File { color, .. }
            | Node::Link { color, .. }
            | Node::Group { color, .. } => color.as_deref(),
        }
    }

    fn display_text(&self) -> String {
        match self {
            Node::Text { text, .. } => text.clone(),
            Node::File { file, .. } => file.clone(),
            Node::Link { url, .. } => url.clone(),
            Node::Group { label, .. } => label.clone().unwrap_or_default(),
        }
    }

    fn is_group(&self) -> bool {
        matches!(self, Node::Group { .. })
    }
}

fn resolve_color(color: Option<&str>) -> &str {
    match color {
        Some("1") => "#fb464c",
        Some("2") => "#e9973f",
        Some("3") => "#e0de71",
        Some("4") => "#44cf6e",
        Some("5") => "#53dfdd",
        Some("6") => "#a882ff",
        Some(hex) if hex.starts_with('#') => hex,
        _ => "#b4b4b4",
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn wrap_text(text: &str, max_width: i64) -> Vec<String> {
    let char_width = 7.5_f64;
    let max_chars = ((max_width as f64 - 20.0) / char_width).max(1.0) as usize;
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current_line = words[0].to_string();
        for word in &words[1..] {
            if current_line.len() + 1 + word.len() > max_chars {
                lines.push(current_line);
                current_line = word.to_string();
            } else {
                current_line.push(' ');
                current_line.push_str(word);
            }
        }
        lines.push(current_line);
    }
    lines
}

fn side_point(x: i64, y: i64, w: i64, h: i64, side: Option<&str>) -> (f64, f64) {
    match side {
        Some("top") => (x as f64 + w as f64 / 2.0, y as f64),
        Some("bottom") => (x as f64 + w as f64 / 2.0, y as f64 + h as f64),
        Some("left") => (x as f64, y as f64 + h as f64 / 2.0),
        Some("right") => (x as f64 + w as f64, y as f64 + h as f64 / 2.0),
        _ => (x as f64 + w as f64 / 2.0, y as f64 + h as f64 / 2.0),
    }
}

fn best_side(
    from: (i64, i64, i64, i64),
    to: (i64, i64, i64, i64),
) -> (&'static str, &'static str) {
    let (fx, fy, fw, fh) = from;
    let (tx, ty, tw, th) = to;
    let fcx = fx as f64 + fw as f64 / 2.0;
    let fcy = fy as f64 + fh as f64 / 2.0;
    let tcx = tx as f64 + tw as f64 / 2.0;
    let tcy = ty as f64 + th as f64 / 2.0;
    let dx = tcx - fcx;
    let dy = tcy - fcy;

    if dx.abs() > dy.abs() {
        if dx > 0.0 {
            ("right", "left")
        } else {
            ("left", "right")
        }
    } else if dy > 0.0 {
        ("bottom", "top")
    } else {
        ("top", "bottom")
    }
}

fn render_svg(canvas: &Canvas, padding: i64) -> Result<String> {
    let mut svg = String::new();

    let node_map: HashMap<&str, &Node> = canvas.nodes.iter().map(|n| (n.id(), n)).collect();

    if canvas.nodes.is_empty() {
        return Ok(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"></svg>"#.into(),
        );
    }

    let mut min_x = i64::MAX;
    let mut min_y = i64::MAX;
    let mut max_x = i64::MIN;
    let mut max_y = i64::MIN;
    for node in &canvas.nodes {
        let (x, y, w, h) = node.bounds();
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    }

    let vw = max_x - min_x + padding * 2;
    let vh = max_y - min_y + padding * 2;
    let ox = -min_x + padding;
    let oy = -min_y + padding;

    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{vw}" height="{vh}" viewBox="0 0 {vw} {vh}">"#,
    )?;

    writeln!(svg, "<defs>")?;
    writeln!(
        svg,
        r#"<marker id="arrowhead" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto">"#
    )?;
    writeln!(svg, "  <polygon points=\"0 0, 10 3.5, 0 7\" fill=\"#888\" />")?;
    writeln!(svg, "</marker>")?;
    writeln!(svg, "</defs>")?;
    writeln!(svg, r#"<style>"#)?;
    writeln!(
        svg,
        r#"  .node-rect {{ fill: #fff; stroke-width: 2; rx: 8; ry: 8; }}"#
    )?;
    writeln!(
        svg,
        r#"  .group-rect {{ fill: none; stroke-dasharray: 6 3; stroke-width: 2; rx: 8; ry: 8; }}"#
    )?;
    writeln!(
        svg,
        r#"  .node-text {{ font-family: -apple-system, "Segoe UI", sans-serif; font-size: 14px; fill: #1a1a1a; }}"#
    )?;
    writeln!(
        svg,
        r#"  .group-label {{ font-family: -apple-system, "Segoe UI", sans-serif; font-size: 13px; fill: #666; font-weight: 600; }}"#
    )?;
    writeln!(
        svg,
        r#"  .edge-label {{ font-family: -apple-system, "Segoe UI", sans-serif; font-size: 12px; fill: #666; }}"#
    )?;
    writeln!(svg, r#"  .edge-line {{ fill: none; stroke-width: 2; }}"#)?;
    writeln!(svg, "</style>")?;

    // Groups first (rendered below)
    for node in &canvas.nodes {
        if node.is_group() {
            render_node(&mut svg, node, ox, oy)?;
        }
    }
    for node in &canvas.nodes {
        if !node.is_group() {
            render_node(&mut svg, node, ox, oy)?;
        }
    }

    // Edges
    for edge in &canvas.edges {
        let from = node_map.get(edge.from_node.as_str());
        let to = node_map.get(edge.to_node.as_str());
        if let (Some(from_node), Some(to_node)) = (from, to) {
            let from_bounds = from_node.bounds();
            let to_bounds = to_node.bounds();

            let (auto_from_side, auto_to_side) = best_side(from_bounds, to_bounds);
            let from_side = edge.from_side.as_deref().unwrap_or(auto_from_side);
            let to_side = edge.to_side.as_deref().unwrap_or(auto_to_side);

            let (x1, y1) = side_point(
                from_bounds.0 + ox,
                from_bounds.1 + oy,
                from_bounds.2,
                from_bounds.3,
                Some(from_side),
            );
            let (x2, y2) = side_point(
                to_bounds.0 + ox,
                to_bounds.1 + oy,
                to_bounds.2,
                to_bounds.3,
                Some(to_side),
            );

            let color = resolve_color(edge.color.as_deref());
            let color = if color == "#b4b4b4" { "#888" } else { color };

            let (cx1, cy1, cx2, cy2) =
                compute_control_points(x1, y1, x2, y2, from_side, to_side);

            let from_end = edge.from_end.as_deref().unwrap_or("none");
            let to_end = edge.to_end.as_deref().unwrap_or("arrow");

            let cid = &color[1..];

            // Per-color arrowhead markers
            writeln!(
                svg,
                r#"<defs><marker id="ah-{cid}" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto"><polygon points="0 0, 10 3.5, 0 7" fill="{color}" /></marker>"#,
            )?;
            writeln!(
                svg,
                r#"<marker id="ah-{cid}-rev" markerWidth="10" markerHeight="7" refX="0" refY="3.5" orient="auto"><polygon points="10 0, 0 3.5, 10 7" fill="{color}" /></marker></defs>"#,
            )?;

            let mut marker = String::new();
            if to_end == "arrow" {
                write!(marker, r#" marker-end="url(#ah-{cid})""#)?;
            }
            if from_end == "arrow" {
                write!(marker, r#" marker-start="url(#ah-{cid}-rev)""#)?;
            }

            writeln!(
                svg,
                r#"<path class="edge-line" d="M {x1} {y1} C {cx1} {cy1}, {cx2} {cy2}, {x2} {y2}" stroke="{color}"{marker} />"#,
            )?;

            if let Some(ref label) = edge.label {
                let mx = (x1 + x2) / 2.0;
                let my = (y1 + y2) / 2.0;
                writeln!(
                    svg,
                    r#"<rect x="{rx}" y="{ry}" width="{rw}" height="20" rx="4" fill="white" opacity="0.85" />"#,
                    rx = mx - (label.len() as f64 * 3.5) - 4.0,
                    ry = my - 12.0,
                    rw = label.len() as f64 * 7.0 + 8.0,
                )?;
                writeln!(
                    svg,
                    r#"<text class="edge-label" x="{mx}" y="{my}" text-anchor="middle">{}</text>"#,
                    escape_xml(label),
                )?;
            }
        }
    }

    writeln!(svg, "</svg>")?;
    Ok(svg)
}

fn compute_control_points(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    from_side: &str,
    to_side: &str,
) -> (f64, f64, f64, f64) {
    let dist = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
    let offset = (dist * 0.4).max(30.0).min(150.0);

    let (cx1, cy1) = match from_side {
        "top" => (x1, y1 - offset),
        "bottom" => (x1, y1 + offset),
        "left" => (x1 - offset, y1),
        "right" => (x1 + offset, y1),
        _ => (x1, y1),
    };
    let (cx2, cy2) = match to_side {
        "top" => (x2, y2 - offset),
        "bottom" => (x2, y2 + offset),
        "left" => (x2 - offset, y2),
        "right" => (x2 + offset, y2),
        _ => (x2, y2),
    };
    (cx1, cy1, cx2, cy2)
}

fn render_node(svg: &mut String, node: &Node, ox: i64, oy: i64) -> Result<()> {
    let (x, y, w, h) = node.bounds();
    let rx = x + ox;
    let ry = y + oy;
    let color = resolve_color(node.color());

    if node.is_group() {
        writeln!(
            svg,
            r#"<rect class="group-rect" x="{rx}" y="{ry}" width="{w}" height="{h}" stroke="{color}" />"#,
        )?;
        if let Node::Group {
            label: Some(label),
            ..
        } = node
        {
            writeln!(
                svg,
                r#"<text class="group-label" x="{}" y="{}">{}</text>"#,
                rx + 12,
                ry - 8,
                escape_xml(label),
            )?;
        }
    } else {
        writeln!(
            svg,
            r#"<rect class="node-rect" x="{rx}" y="{ry}" width="{w}" height="{h}" stroke="{color}" />"#,
        )?;

        let text = node.display_text();
        let lines = wrap_text(&text, w);
        let line_height = 20.0;
        let total_text_height = lines.len() as f64 * line_height;
        let start_y = ry as f64 + (h as f64 - total_text_height) / 2.0 + 14.0;

        for (i, line) in lines.iter().enumerate() {
            let ly = start_y + i as f64 * line_height;
            if ly > (ry + h) as f64 - 4.0 {
                break;
            }
            writeln!(
                svg,
                r#"<text class="node-text" x="{}" y="{ly}">{}</text>"#,
                rx + 12,
                escape_xml(line),
            )?;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    latest::ensure_latest(
        env!("LATEST_GIT_HASH"),
        env!("LATEST_SOURCE_HASH"),
        env!("CARGO_MANIFEST_DIR"),
    );

    let cli = Cli::parse();

    let content = fs::read_to_string(&cli.input)
        .with_context(|| format!("Failed to read {}", cli.input.display()))?;

    let canvas: Canvas =
        serde_json::from_str(&content).with_context(|| "Failed to parse canvas JSON")?;

    let svg = render_svg(&canvas, cli.padding)?;

    fs::write(&cli.output, &svg)
        .with_context(|| format!("Failed to write {}", cli.output.display()))?;

    eprintln!(
        "Rendered {} nodes and {} edges to {}",
        canvas.nodes.len(),
        canvas.edges.len(),
        cli.output.display()
    );

    Ok(())
}
