//! ER diagram port from Beautiful Mermaid 1.1.3 (MIT, Copyright 2026 Craft Docs).
use crate::{
    class::{MONO, edge_value, mono_width, number, root, route},
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
pub struct Attribute {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub keys: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub label: String,
    pub attributes: Vec<Attribute>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Relationship {
    pub entity1: String,
    pub entity2: String,
    pub cardinality1: String,
    pub cardinality2: String,
    pub label: String,
    pub identifying: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagram {
    pub entities: Vec<Entity>,
    pub relationships: Vec<Relationship>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionedEntity {
    #[serde(flatten)]
    pub entity: Entity,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub header_height: f64,
    pub row_height: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionedRelationship {
    #[serde(flatten)]
    pub relationship: Relationship,
    pub points: Vec<Point>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionedDiagram {
    pub width: f64,
    pub height: f64,
    pub entities: Vec<PositionedEntity>,
    pub relationships: Vec<PositionedRelationship>,
}
static DECL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\S+)\s*\{$").unwrap());
static REL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\S+)\s+([|o}{]+)(--|\.\.)([|o}{]+)\s+(\S+)\s*:\s*(.+)$").unwrap()
});
static ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^(\S+)\s+(\S+)(?:\s+(.+))?$"#).unwrap());
static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]*)""#).unwrap());
fn ensure(entities: &mut Vec<Entity>, id: &str) -> usize {
    if let Some(i) = entities.iter().position(|e| e.id == id) {
        i
    } else {
        entities.push(Entity {
            id: id.into(),
            label: id.into(),
            attributes: vec![],
        });
        entities.len() - 1
    }
}
fn cardinality(s: &str) -> Result<String, String> {
    let mut chars: Vec<_> = s.chars().collect();
    chars.sort_unstable();
    Ok(match chars.iter().collect::<String>().as_str() {
        "||" => "one",
        "o|" => "zero-one",
        "|}" | "{|" => "many",
        "o{" => "zero-many",
        _ => return Err(format!("Unsupported cardinality {s}")),
    }
    .into())
}
fn attribute(line: &str) -> Result<Attribute, String> {
    let c = ATTR
        .captures(line)
        .ok_or("Expected attribute type and name")?;
    let rest = c.get(3).map(|v| v.as_str()).unwrap_or("");
    let comment = COMMENT.captures(rest).map(|c| normalize_br_tags(&c[1]));
    let remaining = COMMENT.replace(rest, "");
    let mut keys = vec![];
    for key in remaining.split_whitespace() {
        let key = key.to_uppercase();
        if !["PK", "FK", "UK"].contains(&key.as_str()) {
            return Err(format!("Unsupported attribute key {key}"));
        }
        keys.push(key);
    }
    Ok(Attribute {
        kind: c[1].into(),
        name: c[2].into(),
        keys,
        comment,
    })
}
/// Parse entity attributes and identifying/non-identifying cardinality edges.
pub fn parse(source: &str) -> Result<Diagram, String> {
    if source.len() > 65_536 {
        return Err("ER diagram exceeds 64 KiB".into());
    }
    let mut lines = source
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim().starts_with("%%"));
    if lines.next().map(|(_, l)| l.trim()) != Some("erDiagram") {
        return Err("Expected erDiagram".into());
    }
    let mut d = Diagram {
        entities: vec![],
        relationships: vec![],
    };
    let mut current: Option<usize> = None;
    for (line_number, line) in lines {
        let line = line.trim();
        let result = (|| -> Result<(), String> {
            if let Some(i) = current {
                if line == "}" {
                    current = None;
                } else {
                    if d.entities[i].attributes.len() >= 200 {
                        return Err("At most 200 attributes per entity".into());
                    }
                    d.entities[i].attributes.push(attribute(line)?);
                }
                return Ok(());
            }
            if let Some(c) = DECL.captures(line) {
                current = Some(ensure(&mut d.entities, &c[1]));
                return Ok(());
            }
            if let Some(c) = REL.captures(line) {
                let one = cardinality(&c[2])?;
                let two = cardinality(&c[4])?;
                ensure(&mut d.entities, &c[1]);
                ensure(&mut d.entities, &c[5]);
                d.relationships.push(Relationship {
                    entity1: c[1].into(),
                    entity2: c[5].into(),
                    cardinality1: one,
                    cardinality2: two,
                    label: normalize_br_tags(c[6].trim().trim_matches(['"', '\''])),
                    identifying: &c[3] == "--",
                });
                return Ok(());
            }
            Err(format!("Unsupported ER syntax: {line}"))
        })();
        result.map_err(|e| format!("Line {}: {e}", line_number + 1))?;
        if d.entities.len() > 100 || d.relationships.len() > 200 {
            return Err("ER diagram exceeds 100 entities or 200 relationships".into());
        }
    }
    if current.is_some() {
        return Err("Unclosed entity body".into());
    }
    if d.entities.is_empty() {
        return Err("ER diagram is empty".into());
    }
    Ok(d)
}
fn size(e: &Entity) -> (f64, f64) {
    let max = e
        .attributes
        .iter()
        .map(|a| {
            mono_width(&format!(
                "{}  {}{}",
                a.kind,
                a.name,
                if a.keys.is_empty() {
                    String::new()
                } else {
                    format!("  {}", a.keys.join(","))
                }
            )) + 28.0
        })
        .fold(
            140.0f64.max(measure_text_width(&e.label, 13.0, 500) + 28.0),
            f64::max,
        );
    (max, 34.0 + e.attributes.len().max(1) as f64 * 22.0)
}
pub fn elk_input(d: &Diagram) -> Value {
    let mut root_id = String::from("root");
    while d.entities.iter().any(|node| node.id == root_id) {
        root_id.push('_');
    }
    json!({"id":root_id,"layoutOptions":{"elk.algorithm":"layered","elk.direction":"RIGHT","elk.spacing.nodeNode":"70","elk.layered.spacing.nodeNodeBetweenLayers":"90","elk.padding":"[top=40,left=40,bottom=40,right=40]","elk.edgeRouting":"ORTHOGONAL","elk.edgeLabels.placement":"CENTER"},"children":d.entities.iter().map(|e|{let(w,h)=size(e);json!({"id":e.id,"width":w,"height":h})}).collect::<Vec<_>>(),"edges":d.relationships.iter().enumerate().map(|(i,r)|edge_value(i,&r.entity1,&r.entity2,Some(&r.label))).collect::<Vec<_>>()})
}
pub fn layout(source: &str, _options: &RenderOptions) -> Result<PositionedDiagram, String> {
    let d = parse(source)?;
    let raw: Value =
        serde_json::from_str(&elk_layout::layout_json(&elk_input(&d).to_string(), "{}")?)
            .map_err(|e| e.to_string())?;
    let mut entities = vec![];
    for node in raw["children"].as_array().ok_or("Missing entities")? {
        let entity = d
            .entities
            .iter()
            .find(|e| node["id"] == e.id)
            .ok_or("Unknown entity")?;
        entities.push(PositionedEntity {
            entity: entity.clone(),
            x: number(node, "x")?,
            y: number(node, "y")?,
            width: number(node, "width")?,
            height: number(node, "height")?,
            header_height: 34.0,
            row_height: 22.0,
        });
    }
    let mut relationships = vec![];
    for (i, edge) in raw["edges"]
        .as_array()
        .ok_or("Missing relationships")?
        .iter()
        .enumerate()
    {
        relationships.push(PositionedRelationship {
            relationship: d
                .relationships
                .get(i)
                .ok_or("Unknown relationship")?
                .clone(),
            points: route(edge)?,
        });
    }
    Ok(PositionedDiagram {
        width: number(&raw, "width")?,
        height: number(&raw, "height")?,
        entities,
        relationships,
    })
}
fn midpoint(points: &[Point]) -> Point {
    let total: f64 = points
        .windows(2)
        .map(|p| (p[1].x - p[0].x).hypot(p[1].y - p[0].y))
        .sum();
    if total == 0.0 {
        return points[0];
    }
    let mut walked = 0.0;
    for p in points.windows(2) {
        let dx = p[1].x - p[0].x;
        let dy = p[1].y - p[0].y;
        let len = dx.hypot(dy);
        if walked + len >= total / 2.0 {
            let t = if len > 0.0 {
                (total / 2.0 - walked) / len
            } else {
                0.0
            };
            return Point {
                x: p[0].x + dx * t,
                y: p[0].y + dy * t,
            };
        }
        walked += len;
    }
    *points.last().unwrap()
}
fn crow(point: Point, toward: Point, card: &str, p: &theme::Palette) -> String {
    let dx = point.x - toward.x;
    let dy = point.y - toward.y;
    let len = dx.hypot(dy);
    if len == 0.0 {
        return String::new();
    }
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);
    let tip = Point {
        x: point.x - ux * 4.0,
        y: point.y - uy * 4.0,
    };
    let back = Point {
        x: point.x - ux * 16.0,
        y: point.y - uy * 16.0,
    };
    let mut s = String::new();
    let line = |s: &mut String, a: Point, b: Point| {
        write!(
            s,
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="1.25"/>"#,
            a.x, a.y, b.x, b.y, p.line
        )
        .unwrap();
    };
    let many = card == "many" || card == "zero-many";
    if !many {
        for off in [0.0, 4.0] {
            let c = Point {
                x: tip.x - ux * off,
                y: tip.y - uy * off,
            };
            line(
                &mut s,
                Point {
                    x: c.x + px * 6.0,
                    y: c.y + py * 6.0,
                },
                Point {
                    x: c.x - px * 6.0,
                    y: c.y - py * 6.0,
                },
            );
        }
    } else {
        for off in [7.0, 0.0, -7.0] {
            line(
                &mut s,
                Point {
                    x: tip.x + px * off,
                    y: tip.y + py * off,
                },
                back,
            );
        }
    }
    if card.starts_with("zero-") {
        let off = if many { 20.0 } else { 12.0 };
        write!(
            s,
            r#"<circle cx="{}" cy="{}" r="4" fill="{}" stroke="{}" stroke-width="1.25"/>"#,
            point.x - ux * off,
            point.y - uy * off,
            p.bg,
            p.line
        )
        .unwrap();
    }
    s
}
/// Render entity tables and crow's-foot relationships to a static SVG.
pub fn render(source: &str, options: &RenderOptions) -> Result<String, String> {
    let d = layout(source, options)?;
    let p = theme::resolve(&options.colors)?;
    let mut s = root(d.width, d.height, options, &p);
    for r in &d.relationships {
        let rel = &r.relationship;
        writeln!(s,r#"<polyline class="er-relationship" data-entity1="{}" data-entity2="{}" data-cardinality1="{}" data-cardinality2="{}" points="{}" fill="none" stroke="{}" stroke-width="1"{}/>"#,escape(&rel.entity1),escape(&rel.entity2),rel.cardinality1,rel.cardinality2,r.points.iter().map(|p|format!("{},{}",p.x,p.y)).collect::<Vec<_>>().join(" "),p.line,if rel.identifying{""}else{" stroke-dasharray=\"6 4\""}).unwrap();
    }
    for e in &d.entities {
        let (x, y, w, h) = (e.x, e.y, e.width, e.height);
        write!(s,r#"<g class="entity" data-id="{}"><rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{}" stroke="{}" stroke-width="1"/><rect x="{x}" y="{y}" width="{w}" height="34" fill="{}" stroke="{}" stroke-width="1"/>"#,escape(&e.entity.id),p.node_fill,p.node_stroke,p.group_header,p.node_stroke).unwrap();
        s.push_str(&text(
            &e.entity.label,
            x + w / 2.0,
            y + 17.0,
            13.0,
            700,
            &p.text,
            true,
        ));
        write!(
            s,
            r#"<line x1="{x}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="0.75"/>"#,
            y + 34.0,
            x + w,
            y + 34.0,
            p.node_stroke
        )
        .unwrap();
        for (i, a) in e.entity.attributes.iter().enumerate() {
            let row_y = y + 45.0 + i as f64 * 22.0;
            if let Some(comment) = &a.comment {
                write!(s, "<g><title>{}</title>", escape(comment)).unwrap();
            }
            let mut key_width = 0.0;
            if !a.keys.is_empty() {
                let label = a.keys.join(",");
                key_width = measure_text_width(&label, 9.0, 600) + 8.0;
                write!(s,r#"<rect x="{}" y="{}" width="{key_width}" height="14" rx="2" ry="2" fill="{}"/>"#,x+6.0,row_y-7.0,p.key_badge).unwrap();
                s.push_str(&text(
                    &label,
                    x + 6.0 + key_width / 2.0,
                    row_y,
                    9.0,
                    600,
                    &p.secondary,
                    true,
                ));
            }
            let type_x = x
                + 8.0
                + if key_width > 0.0 {
                    key_width + 6.0
                } else {
                    0.0
                };
            write!(s,r#"<text x="{type_x}" y="{row_y}" font-family="{MONO}" dy="3.85" font-size="11" font-weight="400"><tspan fill="{}">{}</tspan></text><text x="{}" y="{row_y}" font-family="{MONO}" text-anchor="end" dy="3.85" font-size="11" font-weight="400"><tspan fill="{}">{}</tspan></text>"#,p.text_muted,escape(&a.kind),x+w-8.0,p.secondary,escape(&a.name)).unwrap();
            if a.comment.is_some() {
                s.push_str("</g>");
            }
        }
        if e.entity.attributes.is_empty() {
            write!(s,r#"<text x="{}" y="{}" text-anchor="middle" dy="3.85" font-size="11" fill="{}" font-style="italic">(no attributes)</text>"#,x+w/2.0,y+45.0,p.text_faint).unwrap();
        }
        s.push_str("</g>");
    }
    for r in &d.relationships {
        s.push_str(&crow(
            r.points[0],
            r.points[1],
            &r.relationship.cardinality1,
            &p,
        ));
        s.push_str(&crow(
            r.points[r.points.len() - 1],
            r.points[r.points.len() - 2],
            &r.relationship.cardinality2,
            &p,
        ));
    }
    for r in &d.relationships {
        if !r.relationship.label.is_empty() {
            let mid = midpoint(&r.points);
            let m = measure_multiline_text(&r.relationship.label, 11.0, 400);
            let (w, h) = (m.width + 8.0, m.height + 6.0);
            write!(s,r#"<rect x="{}" y="{}" width="{w}" height="{h}" rx="2" ry="2" fill="{}" stroke="{}" stroke-width="0.5"/>"#,mid.x-w/2.0,mid.y-h/2.0,p.bg,p.inner_stroke).unwrap();
            s.push_str(&text(
                &r.relationship.label,
                mid.x,
                mid.y,
                11.0,
                400,
                &p.text_muted,
                true,
            ));
        }
    }
    s.push_str("</svg>");
    Ok(s)
}
