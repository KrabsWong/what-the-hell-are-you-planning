//! SVG geometry and typography ported from Beautiful Mermaid 1.1.3.
//!
//! Copyright (c) 2026 Craft Docs, MIT. See THIRD_PARTY_NOTICES.md.
//! Colors are concrete RGB values, and fonts use local fallback instead of imports.
use crate::{
    metrics::measure_multiline_text,
    theme::{self, Palette},
    types::*,
};
use regex::Regex;
use std::{collections::BTreeMap, fmt::Write, sync::LazyLock};

pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn style_color(style: &Style, key: &str, default: &str) -> Result<String, String> {
    match style.get(key) {
        Some(value) => theme::color(value),
        None => Ok(default.into()),
    }
}
fn stroke_width(style: &Style, default: f64) -> Result<f64, String> {
    let Some(value) = style.get("stroke-width") else {
        return Ok(default);
    };
    let width: f64 = value
        .trim_end_matches("px")
        .parse()
        .map_err(|_| "Invalid stroke width.")?;
    if !width.is_finite() || width < 0.0 {
        return Err("Stroke width must be finite and nonnegative.".into());
    }
    Ok(width)
}

static FORMATTING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<(/)?(b|strong|i|em|u|s|del)\s*>").unwrap());
fn line_content(line: &str) -> String {
    let mut out = String::new();
    let mut active = [false; 4];
    let mut previous = 0;
    let emit = |out: &mut String, text: &str, active: [bool; 4]| {
        if text.is_empty() {
            return;
        }
        if !active.iter().any(|v| *v) {
            out.push_str(&escape(text));
            return;
        }
        out.push_str("<tspan");
        if active[0] {
            out.push_str(" font-weight=\"bold\"")
        }
        if active[1] {
            out.push_str(" font-style=\"italic\"")
        }
        if active[2] || active[3] {
            let mut decorations = Vec::new();
            if active[2] {
                decorations.push("underline")
            }
            if active[3] {
                decorations.push("line-through")
            }
            write!(out, " text-decoration=\"{}\"", decorations.join(" ")).unwrap();
        }
        write!(out, ">{}</tspan>", escape(text)).unwrap();
    };
    for captures in FORMATTING.captures_iter(line) {
        let matched = captures.get(0).unwrap();
        emit(&mut out, &line[previous..matched.start()], active);
        let index = match captures[2].to_ascii_lowercase().as_str() {
            "b" | "strong" => 0,
            "i" | "em" => 1,
            "u" => 2,
            _ => 3,
        };
        active[index] = captures.get(1).is_none();
        previous = matched.end();
    }
    emit(&mut out, &line[previous..], active);
    out
}
pub(crate) fn text(
    text: &str,
    x: f64,
    y: f64,
    size: f64,
    weight: u16,
    color: &str,
    center: bool,
) -> String {
    let anchor = if center {
        " text-anchor=\"middle\""
    } else {
        ""
    };
    let attrs = format!("font-size=\"{size}\" font-weight=\"{weight}\" fill=\"{color}\"{anchor}");
    let lines: Vec<_> = text.split('\n').collect();
    if lines.len() == 1 {
        return format!(
            "<text x=\"{x}\" y=\"{y}\" {attrs} dy=\"{}\">{}</text>",
            size * 0.35,
            line_content(text)
        );
    }
    let mut out = format!("<text x=\"{x}\" y=\"{y}\" {attrs}>");
    let line_height = size * 1.3;
    for (i, line) in lines.iter().enumerate() {
        let dy = if i == 0 {
            -((lines.len() - 1) as f64 / 2.0) * line_height + size * 0.35
        } else {
            line_height
        };
        write!(
            out,
            "<tspan x=\"{x}\" dy=\"{dy}\">{}</tspan>",
            line_content(line)
        )
        .unwrap();
    }
    out.push_str("</text>");
    out
}
fn marker(out: &mut String, id: &str, color: &str) {
    write!(out,r#"<marker id="{id}" markerWidth="8" markerHeight="5" refX="7" refY="2.5" orient="auto"><polygon points="0 0, 8 2.5, 0 5" fill="{color}" stroke="{color}" stroke-width="0.75" stroke-linejoin="round"/></marker><marker id="{id}-start" markerWidth="8" markerHeight="5" refX="1" refY="2.5" orient="auto-start-reverse"><polygon points="8 0, 0 2.5, 8 5" fill="{color}" stroke="{color}" stroke-width="0.75" stroke-linejoin="round"/></marker>"#).unwrap();
}
fn group(out: &mut String, g: &PositionedGroup, p: &Palette) {
    write!(out,r#"<g class="subgraph" data-id="{}" data-label="{}"><rect x="{}" y="{}" width="{}" height="{}" rx="0" ry="0" fill="{}" stroke="{}" stroke-width="1"/><rect x="{}" y="{}" width="{}" height="28" rx="0" ry="0" fill="{}" stroke="{}" stroke-width="1"/>"#,escape(&g.id),escape(&g.label),g.x,g.y,g.width,g.height,p.bg,p.node_stroke,g.x,g.y,g.width,p.group_header,p.node_stroke).unwrap();
    out.push_str(&text(
        &g.label,
        g.x + 12.0,
        g.y + 14.0,
        12.0,
        600,
        &p.secondary,
        false,
    ));
    for child in &g.children {
        group(out, child, p)
    }
    out.push_str("</g>");
}
fn midpoint(points: &[Point]) -> Point {
    let Some(first) = points.first() else {
        return Point { x: 0.0, y: 0.0 };
    };
    let distance = |a: Point, b: Point| (b.x - a.x).hypot(b.y - a.y);
    let mut remaining = points.windows(2).map(|s| distance(s[0], s[1])).sum::<f64>() / 2.0;
    for pair in points.windows(2) {
        let length = distance(pair[0], pair[1]);
        if length > 0.0 && remaining <= length {
            let t = remaining / length;
            return Point {
                x: pair[0].x + t * (pair[1].x - pair[0].x),
                y: pair[0].y + t * (pair[1].y - pair[0].y),
            };
        }
        remaining -= length;
    }
    points.last().copied().unwrap_or(*first)
}
fn node_shape(n: &PositionedNode, p: &Palette) -> Result<String, String> {
    let (x, y, w, h) = (n.x, n.y, n.width, n.height);
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let fill = style_color(&n.inline_style, "fill", &p.node_fill)?;
    let stroke = style_color(&n.inline_style, "stroke", &p.node_stroke)?;
    let sw = stroke_width(&n.inline_style, 0.75)?;
    let attrs = format!("fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{sw}\"");
    let rect = |r: f64| {
        format!(r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" ry="{r}" {attrs}/>"#)
    };
    let polygon = |points: &[(f64, f64)]| {
        format!(
            "<polygon points=\"{}\" {attrs}/>",
            points
                .iter()
                .map(|(x, y)| format!("{x},{y}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    Ok(match n.shape.as_str() {
        "rectangle" => rect(0.0),
        "rounded" => rect(6.0),
        "stadium" => rect(h / 2.0),
        "diamond" => polygon(&[(cx, y), (x + w, cy), (cx, y + h), (x, cy)]),
        "circle" => format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{}" {attrs}/>"#,
            w.min(h) / 2.0
        ),
        "doublecircle" => format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{}" {attrs}/><circle cx="{cx}" cy="{cy}" r="{}" {attrs}/>"#,
            w.min(h) / 2.0,
            w.min(h) / 2.0 - 5.0
        ),
        "subroutine" => format!(
            r#"{}<line x1="{}" y1="{y}" x2="{}" y2="{}" stroke="{stroke}" stroke-width="{sw}"/><line x1="{}" y1="{y}" x2="{}" y2="{}" stroke="{stroke}" stroke-width="{sw}"/>"#,
            rect(0.0),
            x + 8.0,
            x + 8.0,
            y + h,
            x + w - 8.0,
            x + w - 8.0,
            y + h
        ),
        "hexagon" => polygon(&[
            (x + h / 4.0, y),
            (x + w - h / 4.0, y),
            (x + w, cy),
            (x + w - h / 4.0, y + h),
            (x + h / 4.0, y + h),
            (x, cy),
        ]),
        "asymmetric" => polygon(&[
            (x + 12.0, y),
            (x + w, y),
            (x + w, y + h),
            (x + 12.0, y + h),
            (x, cy),
        ]),
        "trapezoid" => polygon(&[
            (x + w * 0.15, y),
            (x + w * 0.85, y),
            (x + w, y + h),
            (x, y + h),
        ]),
        "trapezoid-alt" => polygon(&[
            (x, y),
            (x + w, y),
            (x + w * 0.85, y + h),
            (x + w * 0.15, y + h),
        ]),
        "cylinder" => format!(
            r#"<rect x="{x}" y="{}" width="{w}" height="{}" fill="{fill}" stroke="none"/><line x1="{x}" y1="{}" x2="{x}" y2="{}" stroke="{stroke}" stroke-width="{sw}"/><line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{stroke}" stroke-width="{sw}"/><ellipse cx="{cx}" cy="{}" rx="{}" ry="7" {attrs}/><ellipse cx="{cx}" cy="{}" rx="{}" ry="7" {attrs}/>"#,
            y + 7.0,
            h - 14.0,
            y + 7.0,
            y + h - 7.0,
            x + w,
            y + 7.0,
            x + w,
            y + h - 7.0,
            y + h - 7.0,
            w / 2.0,
            y + 7.0,
            w / 2.0
        ),
        "state-start" => format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="{}" stroke="none"/>"#,
            w.min(h) / 2.0 - 2.0,
            p.text
        ),
        "state-end" => format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="none" stroke="{}" stroke-width="1.5"/><circle cx="{cx}" cy="{cy}" r="{}" fill="{}" stroke="none"/>"#,
            w.min(h) / 2.0 - 2.0,
            p.text,
            w.min(h) / 2.0 - 6.0,
            p.text
        ),
        _ => return Err(format!("Unsupported node shape: {}", n.shape)),
    })
}

/// Render positioned geometry to standalone SVG with resolved native colors.
///
/// No remote resources are imported. Use a locally installed font for stable
/// raster comparisons; `Inter` falls back to the platform sans-serif font.
pub fn render_svg(graph: &PositionedGraph, options: &RenderOptions) -> Result<String, String> {
    if !graph.width.is_finite()
        || !graph.height.is_finite()
        || graph.width <= 0.0
        || graph.height <= 0.0
    {
        return Err("SVG dimensions must be finite and positive.".into());
    }
    let p = theme::resolve(&options.colors)?;
    let mut out = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="{}" height="{}" font-family="{}, system-ui, sans-serif">"#,
        graph.width,
        graph.height,
        graph.width,
        graph.height,
        escape(&options.font)
    );
    if !options.transparent {
        write!(out, r#"<rect width="100%" height="100%" fill="{}"/>"#, p.bg).unwrap()
    }
    out.push_str("<defs>");
    marker(&mut out, "arrowhead", &p.arrow);
    let mut markers = BTreeMap::new();
    for edge in &graph.edges {
        if let Some(color) = edge.inline_style.get("stroke") {
            let color = theme::color(color)?;
            if !markers.contains_key(&color) {
                let id = format!("arrowhead-custom-{}", markers.len());
                marker(&mut out, &id, &color);
                markers.insert(color, id);
            }
        }
    }
    out.push_str("</defs>");
    for g in &graph.groups {
        group(&mut out, g, &p)
    }
    for edge in &graph.edges {
        if edge.points.len() < 2 {
            continue;
        }
        let color = style_color(&edge.inline_style, "stroke", &p.line)?;
        let width = stroke_width(
            &edge.inline_style,
            if edge.style == "thick" { 2.0 } else { 1.0 },
        )?;
        let id = if edge.inline_style.contains_key("stroke") {
            markers.get(&color).unwrap().as_str()
        } else {
            "arrowhead"
        };
        let points = edge
            .points
            .iter()
            .map(|p| format!("{},{}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ");
        write!(out,r#"<polyline class="edge" data-from="{}" data-to="{}" data-style="{}" data-arrow-start="{}" data-arrow-end="{}" points="{points}" fill="none" stroke="{color}" stroke-width="{width}""#,escape(&edge.source),escape(&edge.target),escape(&edge.style),edge.has_arrow_start,edge.has_arrow_end).unwrap();
        if let Some(label) = &edge.label {
            write!(out, " data-label=\"{}\"", escape(label)).unwrap()
        }
        if edge.style == "dotted" {
            out.push_str(" stroke-dasharray=\"4 4\"")
        }
        if edge.has_arrow_start {
            write!(out, " marker-start=\"url(#{id}-start)\"").unwrap()
        }
        if edge.has_arrow_end {
            write!(out, " marker-end=\"url(#{id})\"").unwrap()
        }
        out.push_str("/>");
    }
    for edge in &graph.edges {
        let Some(label) = edge.label.as_ref().filter(|label| !label.is_empty()) else {
            continue;
        };
        let middle = edge
            .label_position
            .unwrap_or_else(|| midpoint(&edge.points));
        let metrics = measure_multiline_text(label, 11.0, 400);
        let (w, h) = (metrics.width + 16.0, metrics.height + 16.0);
        write!(out,r#"<g class="edge-label" data-from="{}" data-to="{}" data-label="{}"><rect x="{}" y="{}" width="{w}" height="{h}" rx="2" ry="2" fill="{}" stroke="{}" stroke-width="1"/>"#,escape(&edge.source),escape(&edge.target),escape(label),middle.x-w/2.0,middle.y-h/2.0,p.bg,p.inner_stroke).unwrap();
        out.push_str(&text(
            label,
            middle.x,
            middle.y,
            11.0,
            400,
            &p.secondary,
            true,
        ));
        out.push_str("</g>");
    }
    for n in &graph.nodes {
        write!(
            out,
            r#"<g class="node" data-id="{}" data-label="{}" data-shape="{}">"#,
            escape(&n.id),
            escape(&n.label),
            escape(&n.shape)
        )
        .unwrap();
        out.push_str(&node_shape(n, &p)?);
        if !n.label.is_empty() || !matches!(n.shape.as_str(), "state-start" | "state-end") {
            let color = style_color(&n.inline_style, "color", &p.text)?;
            out.push_str(&text(
                &n.label,
                n.x + n.width / 2.0,
                n.y + n.height / 2.0,
                13.0,
                500,
                &color,
                true,
            ));
        }
        out.push_str("</g>");
    }
    out.push_str("</svg>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inline_formatting_is_svg_not_html() {
        assert_eq!(
            line_content("<b>Bold <i>both</i></b>"),
            "<tspan font-weight=\"bold\">Bold </tspan><tspan font-weight=\"bold\" font-style=\"italic\">both</tspan>"
        );
        assert_eq!(
            line_content("<script>alert(1)</script>"),
            "&lt;script&gt;alert(1)&lt;/script&gt;"
        );
    }
    #[test]
    fn multiline_baseline_matches_upstream() {
        let result = text("First\nSecond", 50.0, 30.0, 13.0, 500, "#000000", true);
        let dy: Vec<f64> = Regex::new(r#"dy="([^"]+)""#)
            .unwrap()
            .captures_iter(&result)
            .map(|c| c[1].parse().unwrap())
            .collect();
        assert_eq!(dy.len(), 2);
        assert!((dy[0] + 3.9).abs() < 1e-12);
        assert!((dy[1] - 16.9).abs() < 1e-12);
    }
    #[test]
    fn repeated_edge_points_have_finite_midpoint() {
        let midpoint = midpoint(&[Point { x: 1.0, y: 2.0 }, Point { x: 1.0, y: 2.0 }]);
        assert_eq!(midpoint.x, 1.0);
        assert_eq!(midpoint.y, 2.0);
    }
}
