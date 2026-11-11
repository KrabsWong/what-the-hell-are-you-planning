//! Class diagram port from Beautiful Mermaid 1.1.3 (MIT, Copyright 2026 Craft Docs).
use crate::{
    metrics::{measure_multiline_text, measure_text_width, normalize_br_tags},
    render::{escape, text},
    theme,
    types::{Point, RenderOptions},
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fmt::Write, sync::LazyLock};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub visibility: String,
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_method: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Class {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation: Option<String>,
    pub attributes: Vec<Member>,
    pub methods: Vec<Member>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub marker_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_cardinality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_cardinality: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagram {
    pub classes: Vec<Class>,
    pub relationships: Vec<Relationship>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionedClass {
    #[serde(flatten)]
    pub class: Class,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub header_height: f64,
    pub attr_height: f64,
    pub method_height: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionedRelationship {
    #[serde(flatten)]
    pub relationship: Relationship,
    pub points: Vec<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_position: Option<Point>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionedDiagram {
    pub width: f64,
    pub height: f64,
    pub classes: Vec<PositionedClass>,
    pub relationships: Vec<PositionedRelationship>,
}
static DECL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^class\s+([^\s{}]+?)(?:\s*~(\w+)~)?\s*(\{)?$").unwrap());
static ANNOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<<(\w+)>>$").unwrap());
static INLINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^class\s+(\S+?)\s*\{\s*<<(\w+)>>\s*\}$").unwrap());
static REL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(\S+?)\s+(?:"([^"]*?)"\s+)?(<\|--|<\|\.\.|\*--|o--|-->|--\*|--o|--\|>|\.\.>|\.\.\|>|<--|<\.\.|--)\s+(?:"([^"]*?)"\s+)?(\S+?)(?:\s*:\s*(.+))?$"#).unwrap()
});
static METHOD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+?)\(([^)]*)\)(?:\s*(.+))?$").unwrap());
fn ensure(classes: &mut Vec<Class>, id: &str) -> usize {
    if let Some(i) = classes.iter().position(|c| c.id == id) {
        i
    } else {
        classes.push(Class {
            id: id.into(),
            label: id.into(),
            annotation: None,
            attributes: vec![],
            methods: vec![],
        });
        classes.len() - 1
    }
}
fn member(source: &str) -> Result<Member, String> {
    let mut rest = source.trim().trim_end_matches(';');
    let mut visibility = String::new();
    if let Some(c) = rest.chars().next().filter(|c| "+-#~".contains(*c)) {
        visibility.push(c);
        rest = rest[c.len_utf8()..].trim();
    }
    if rest.is_empty() {
        return Err("Empty class member".into());
    }
    if let Some(c) = METHOD.captures(rest) {
        let name = c[1].trim();
        return Ok(Member {
            visibility,
            name: name.trim_end_matches(['$', '*']).into(),
            kind: c.get(3).map(|v| v.as_str().trim().into()),
            is_static: rest.contains('$'),
            is_abstract: rest.contains('*'),
            is_method: true,
            params: c
                .get(2)
                .map(|v| v.as_str().trim())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        });
    }
    let pieces: Vec<_> = rest.split_whitespace().collect();
    let (kind, name) = if pieces.len() > 1 {
        (Some(pieces[0].into()), pieces[1..].join(" "))
    } else {
        (None, rest.into())
    };
    Ok(Member {
        visibility,
        is_static: name.ends_with('$'),
        is_abstract: name.ends_with('*'),
        name: name.trim_end_matches(['$', '*']).into(),
        kind,
        is_method: false,
        params: None,
    })
}
fn add_member(class: &mut Class, source: &str) -> Result<(), String> {
    let m = member(source)?;
    if m.is_method {
        class.methods.push(m);
    } else {
        class.attributes.push(m);
    }
    if class.attributes.len() + class.methods.len() > 200 {
        return Err("At most 200 members per class".into());
    }
    Ok(())
}
/// Parse the upstream class subset. Namespace declarations are explicitly unsupported.
pub fn parse(source: &str) -> Result<Diagram, String> {
    if source.len() > 65_536 {
        return Err("Class diagram exceeds 64 KiB".into());
    }
    let mut lines = source
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim().starts_with("%%"));
    if lines.next().map(|(_, l)| l.trim()) != Some("classDiagram") {
        return Err("Expected classDiagram".into());
    }
    let mut d = Diagram {
        classes: vec![],
        relationships: vec![],
    };
    let mut current: Option<usize> = None;
    for (line_number, line) in lines {
        let line = line.trim();
        let result = (|| -> Result<(), String> {
            if let Some(i) = current {
                if line == "}" {
                    current = None;
                    return Ok(());
                }
                if let Some(c) = ANNOT.captures(line) {
                    d.classes[i].annotation = Some(c[1].into());
                } else {
                    add_member(&mut d.classes[i], line)?;
                }
                return Ok(());
            }
            if line.starts_with("namespace ") {
                return Err("Class namespaces are not supported by the native renderer".into());
            }
            if let Some(c) = INLINE.captures(line) {
                let i = ensure(&mut d.classes, &c[1]);
                d.classes[i].annotation = Some(c[2].into());
                return Ok(());
            }
            if let Some(c) = DECL.captures(line) {
                let i = ensure(&mut d.classes, &c[1]);
                if let Some(g) = c.get(2) {
                    d.classes[i].label = format!("{}<{}>", &c[1], g.as_str());
                }
                if c.get(3).is_some() {
                    current = Some(i);
                }
                return Ok(());
            }
            if let Some(c) = REL.captures(line) {
                let arrow = &c[3];
                let (kind, at) = match arrow {
                    "<|--" => ("inheritance", "from"),
                    "--|>" => ("inheritance", "to"),
                    "<|.." => ("realization", "from"),
                    "..|>" => ("realization", "to"),
                    "*--" => ("composition", "from"),
                    "--*" => ("composition", "to"),
                    "o--" => ("aggregation", "from"),
                    "--o" => ("aggregation", "to"),
                    "<--" => ("association", "from"),
                    "..>" => ("dependency", "to"),
                    "<.." => ("dependency", "from"),
                    _ => ("association", "to"),
                };
                ensure(&mut d.classes, &c[1]);
                ensure(&mut d.classes, &c[5]);
                d.relationships.push(Relationship {
                    from: c[1].into(),
                    to: c[5].into(),
                    kind: kind.into(),
                    marker_at: at.into(),
                    label: c.get(6).map(|v| normalize_br_tags(v.as_str().trim())),
                    from_cardinality: c
                        .get(2)
                        .filter(|v| !v.as_str().is_empty())
                        .map(|v| normalize_br_tags(v.as_str())),
                    to_cardinality: c
                        .get(4)
                        .filter(|v| !v.as_str().is_empty())
                        .map(|v| normalize_br_tags(v.as_str())),
                });
                return Ok(());
            }
            if let Some((id, rest)) = line.split_once(':')
                && !id.trim().is_empty()
                && !id.trim().contains(char::is_whitespace)
            {
                let i = ensure(&mut d.classes, id.trim());
                add_member(&mut d.classes[i], rest)?;
                return Ok(());
            }
            Err(format!("Unsupported class syntax: {line}"))
        })();
        result.map_err(|e| format!("Line {}: {e}", line_number + 1))?;
        if d.classes.len() > 100 || d.relationships.len() > 200 {
            return Err("Class diagram exceeds 100 classes or 200 relationships".into());
        }
    }
    if current.is_some() {
        return Err("Unclosed class body".into());
    }
    if d.classes.is_empty() {
        return Err("Class diagram is empty".into());
    }
    Ok(d)
}
fn member_string(m: &Member) -> String {
    format!(
        "{}{}{}",
        if m.visibility.is_empty() {
            String::new()
        } else {
            format!("{} ", m.visibility)
        },
        if m.is_method {
            format!("{}({})", m.name, m.params.as_deref().unwrap_or(""))
        } else {
            m.name.clone()
        },
        m.kind
            .as_ref()
            .map(|t| format!(": {t}"))
            .unwrap_or_default()
    )
}
pub(crate) fn mono_width(s: &str) -> f64 {
    s.encode_utf16().count() as f64 * 11.0 * 0.6
}
fn sizes(c: &Class) -> (f64, f64, f64, f64) {
    let header = if c.annotation.is_some() { 48.0 } else { 32.0 };
    let attr = if c.attributes.is_empty() {
        8.0
    } else {
        c.attributes.len() as f64 * 20.0 + 8.0
    };
    let method = if c.methods.is_empty() {
        8.0
    } else {
        c.methods.len() as f64 * 20.0 + 8.0
    };
    let width = c
        .attributes
        .iter()
        .chain(&c.methods)
        .map(|m| mono_width(&member_string(m)) + 16.0)
        .fold(
            120.0f64.max(measure_text_width(&c.label, 13.0, 500) + 16.0),
            f64::max,
        );
    (width, header, attr, method)
}
pub(crate) fn edge_value(id: usize, from: &str, to: &str, label: Option<&str>) -> Value {
    let mut e = json!({"id":format!("e{id}"),"sources":[from],"targets":[to]});
    if let Some(label) = label.filter(|s| !s.is_empty()) {
        let m = measure_multiline_text(label, 11.0, 400);
        e["labels"] = json!([{"text":label,"width":m.width+8.0,"height":m.height+6.0}]);
    }
    e
}
pub fn elk_input(d: &Diagram) -> Value {
    let mut root_id = String::from("root");
    while d.classes.iter().any(|node| node.id == root_id) {
        root_id.push('_');
    }
    json!({"id":root_id,"layoutOptions":{"elk.algorithm":"layered","elk.direction":"DOWN","elk.spacing.nodeNode":"40","elk.layered.spacing.nodeNodeBetweenLayers":"60","elk.padding":"[top=40,left=40,bottom=40,right=40]","elk.edgeRouting":"ORTHOGONAL","elk.edgeLabels.placement":"CENTER"},"children":d.classes.iter().map(|c|{let (w,h,a,m)=sizes(c);json!({"id":c.id,"width":w,"height":h+a+m})}).collect::<Vec<_>>(),"edges":d.relationships.iter().enumerate().map(|(i,r)|edge_value(i,&r.from,&r.to,r.label.as_deref())).collect::<Vec<_>>()})
}
pub(crate) fn number(v: &Value, key: &str) -> Result<f64, String> {
    v[key]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("Missing finite layout value {key}"))
}
pub(crate) fn route(edge: &Value) -> Result<Vec<Point>, String> {
    let sections = edge["sections"]
        .as_array()
        .ok_or("Missing relationship route")?;
    if sections.len() != 1 {
        return Err("Expected one relationship route section".into());
    }
    let s = &sections[0];
    let mut p = vec![Point {
        x: number(&s["startPoint"], "x")?,
        y: number(&s["startPoint"], "y")?,
    }];
    if let Some(b) = s["bendPoints"].as_array() {
        for v in b {
            p.push(Point {
                x: number(v, "x")?,
                y: number(v, "y")?,
            });
        }
    }
    p.push(Point {
        x: number(&s["endPoint"], "x")?,
        y: number(&s["endPoint"], "y")?,
    });
    Ok(p)
}
pub fn layout(source: &str, _options: &RenderOptions) -> Result<PositionedDiagram, String> {
    let d = parse(source)?;
    let raw: Value =
        serde_json::from_str(&elk_layout::layout_json(&elk_input(&d).to_string(), "{}")?)
            .map_err(|e| e.to_string())?;
    let mut classes = vec![];
    for node in raw["children"].as_array().ok_or("Missing classes")? {
        let c = d
            .classes
            .iter()
            .find(|c| node["id"] == c.id)
            .ok_or("Unknown class in layout")?;
        let (_, h, a, m) = sizes(c);
        classes.push(PositionedClass {
            class: c.clone(),
            x: number(node, "x")?,
            y: number(node, "y")?,
            width: number(node, "width")?,
            height: number(node, "height")?,
            header_height: h,
            attr_height: a,
            method_height: m,
        });
    }
    let mut relationships = vec![];
    for (i, edge) in raw["edges"]
        .as_array()
        .ok_or("Missing relationships")?
        .iter()
        .enumerate()
    {
        let r = d.relationships.get(i).ok_or("Unknown relationship")?;
        let position = edge["labels"]
            .as_array()
            .and_then(|a| a.first())
            .map(|l| -> Result<Point, String> {
                Ok(Point {
                    x: number(l, "x")? + number(l, "width")? / 2.0,
                    y: number(l, "y")? + number(l, "height")? / 2.0,
                })
            })
            .transpose()?;
        relationships.push(PositionedRelationship {
            relationship: r.clone(),
            points: route(edge)?,
            label_position: position,
        });
    }
    Ok(PositionedDiagram {
        width: number(&raw, "width")?,
        height: number(&raw, "height")?,
        classes,
        relationships,
    })
}
pub(crate) fn root(width: f64, height: f64, o: &RenderOptions, p: &theme::Palette) -> String {
    let mut s = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="{}, system-ui, sans-serif">"#,
        escape(&o.font)
    );
    if !o.transparent {
        write!(s, r#"<rect width="100%" height="100%" fill="{}"/>"#, p.bg).unwrap();
    }
    s
}
fn markers(p: &theme::Palette) -> String {
    format!(
        r#"<defs><marker id="cls-inherit" markerWidth="12" markerHeight="10" refX="12" refY="5" orient="auto-start-reverse"><polygon points="0 0, 12 5, 0 10" fill="{bg}" stroke="{arrow}" stroke-width="1.5"/></marker><marker id="cls-composition" markerWidth="12" markerHeight="10" refX="0" refY="5" orient="auto-start-reverse"><polygon points="6 0, 12 5, 6 10, 0 5" fill="{arrow}" stroke="{arrow}" stroke-width="1"/></marker><marker id="cls-aggregation" markerWidth="12" markerHeight="10" refX="0" refY="5" orient="auto-start-reverse"><polygon points="6 0, 12 5, 6 10, 0 5" fill="{bg}" stroke="{arrow}" stroke-width="1.5"/></marker><marker id="cls-arrow" markerWidth="8" markerHeight="6" refX="8" refY="3" orient="auto-start-reverse"><polyline points="0 0, 8 3, 0 6" fill="none" stroke="{arrow}" stroke-width="1.5"/></marker></defs>"#,
        bg = p.bg,
        arrow = p.arrow
    )
}
pub(crate) const MONO: &str = "'JetBrains Mono', 'SF Mono', 'Fira Code', ui-monospace, monospace";
fn member_svg(m: &Member, x: f64, y: f64, p: &theme::Palette) -> String {
    let mut s = format!(
        r#"<text x="{x}" y="{y}" font-family="{MONO}" dy="3.85" font-size="11" font-weight="400"{}{}>"#,
        if m.is_abstract {
            " font-style=\"italic\""
        } else {
            ""
        },
        if m.is_static {
            " text-decoration=\"underline\""
        } else {
            ""
        }
    );
    if !m.visibility.is_empty() {
        write!(
            s,
            r#"<tspan fill="{}">{} </tspan>"#,
            p.text_faint,
            escape(&m.visibility)
        )
        .unwrap();
    }
    write!(
        s,
        r#"<tspan fill="{}">{}</tspan>"#,
        p.secondary,
        escape(&if m.is_method {
            format!("{}({})", m.name, m.params.as_deref().unwrap_or(""))
        } else {
            m.name.clone()
        })
    )
    .unwrap();
    if let Some(t) = &m.kind {
        write!(
            s,
            r#"<tspan fill="{}">: </tspan><tspan fill="{}">{}</tspan>"#,
            p.text_faint,
            p.text_muted,
            escape(t)
        )
        .unwrap();
    }
    s.push_str("</text>");
    s
}
fn offset(a: Point, b: Point) -> Point {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    if dx.abs() > dy.abs() {
        Point {
            x: if dx > 0.0 { 14.0 } else { -14.0 },
            y: -10.0,
        }
    } else {
        Point {
            x: -14.0,
            y: if dy > 0.0 { 14.0 } else { -14.0 },
        }
    }
}
/// Render class compartments and UML endpoint markers as native SVG.
pub fn render(source: &str, options: &RenderOptions) -> Result<String, String> {
    let d = layout(source, options)?;
    let p = theme::resolve(&options.colors)?;
    let mut s = root(d.width, d.height, options, &p);
    s.push_str(&markers(&p));
    for r in &d.relationships {
        let rel = &r.relationship;
        let marker = match rel.kind.as_str() {
            "inheritance" | "realization" => "cls-inherit",
            "composition" => "cls-composition",
            "aggregation" => "cls-aggregation",
            _ => "cls-arrow",
        };
        write!(s,r#"<polyline class="class-relationship" data-from="{}" data-to="{}" data-type="{}" data-marker-at="{}" points="{}" fill="none" stroke="{}" stroke-width="1"{} marker-{}="url(#{marker})"/>"#,escape(&rel.from),escape(&rel.to),rel.kind,rel.marker_at,r.points.iter().map(|p|format!("{},{}",p.x,p.y)).collect::<Vec<_>>().join(" "),p.line,if rel.kind=="dependency"||rel.kind=="realization"{" stroke-dasharray=\"6 4\""}else{""},if rel.marker_at=="from"{"start"}else{"end"}).unwrap();
    }
    for c in &d.classes {
        let (x, y, w, h) = (c.x, c.y, c.width, c.height);
        write!(s,r#"<g class="class-node" data-id="{}"><rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{}" stroke="{}" stroke-width="1"/><rect x="{x}" y="{y}" width="{w}" height="{}" fill="{}" stroke="{}" stroke-width="1"/>"#,escape(&c.class.id),p.node_fill,p.node_stroke,c.header_height,p.group_header,p.node_stroke).unwrap();
        let mut name_y = y + c.header_height / 2.0;
        if let Some(a) = &c.class.annotation {
            write!(s,r#"<text x="{}" y="{}" text-anchor="middle" dy="3.5" font-size="10" font-weight="500" font-style="italic" fill="{}">&lt;&lt;{}&gt;&gt;</text>"#,x+w/2.0,y+12.0,p.text_muted,escape(a)).unwrap();
            name_y += 6.0;
        }
        s.push_str(&text(
            &c.class.label,
            x + w / 2.0,
            name_y,
            13.0,
            700,
            &p.text,
            true,
        ));
        let attr_top = y + c.header_height;
        let method_top = attr_top + c.attr_height;
        for top in [attr_top, method_top] {
            write!(
                s,
                r#"<line x1="{x}" y1="{top}" x2="{}" y2="{top}" stroke="{}" stroke-width="0.75"/>"#,
                x + w,
                p.node_stroke
            )
            .unwrap();
        }
        for (members, top) in [
            (&c.class.attributes, attr_top),
            (&c.class.methods, method_top),
        ] {
            for (i, m) in members.iter().enumerate() {
                s.push_str(&member_svg(m, x + 8.0, top + 14.0 + i as f64 * 20.0, &p));
            }
        }
        s.push_str("</g>");
    }
    for r in &d.relationships {
        let rel = &r.relationship;
        if let Some(label) = &rel.label {
            let pos = r.label_position.unwrap_or(r.points[r.points.len() / 2]);
            s.push_str(&text(
                label,
                pos.x,
                pos.y - 8.0,
                11.0,
                400,
                &p.text_muted,
                true,
            ));
        }
        for (label, i, j) in [
            (&rel.from_cardinality, 0, 1),
            (&rel.to_cardinality, r.points.len() - 1, r.points.len() - 2),
        ] {
            if let Some(label) = label {
                let a = r.points[i];
                let delta = offset(a, r.points[j]);
                s.push_str(&text(
                    label,
                    a.x + delta.x,
                    a.y + delta.y,
                    11.0,
                    400,
                    &p.text_muted,
                    true,
                ));
            }
        }
    }
    s.push_str("</svg>");
    Ok(s)
}
