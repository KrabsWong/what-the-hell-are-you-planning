//! Sequence diagrams ported from Beautiful Mermaid 1.1.3 (MIT, Craft Docs).
//! See licenses/Beautiful-Mermaid-MIT.txt for the retained upstream notice.
use crate::{
    RenderOptions,
    metrics::{measure_text_width, normalize_br_tags},
    render::{escape, text},
    theme,
};
use regex::Regex;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    sync::LazyLock,
};

#[derive(Clone, Debug)]
struct Actor {
    id: String,
    label: String,
    kind: String,
}
#[derive(Clone, Debug)]
struct Message {
    from: String,
    to: String,
    label: String,
    line: String,
    head: String,
    activate: bool,
    deactivate: bool,
}
#[derive(Clone, Debug)]
struct Block {
    kind: String,
    label: String,
    start: usize,
    end: usize,
    dividers: Vec<(usize, String)>,
}
#[derive(Clone, Debug)]
struct Note {
    actors: Vec<String>,
    text: String,
    position: String,
    after: isize,
}
#[derive(Default)]
struct Diagram {
    actors: Vec<Actor>,
    messages: Vec<Message>,
    blocks: Vec<Block>,
    notes: Vec<Note>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PositionedSequence {
    pub width: f64,
    pub height: f64,
    pub actors: Vec<PositionedActor>,
    pub lifelines: Vec<Lifeline>,
    pub messages: Vec<PositionedMessage>,
    pub activations: Vec<Activation>,
    pub blocks: Vec<PositionedBlock>,
    pub notes: Vec<PositionedNote>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PositionedActor {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lifeline {
    pub actor_id: String,
    pub x: f64,
    pub top_y: f64,
    pub bottom_y: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionedMessage {
    pub from: String,
    pub to: String,
    pub label: String,
    pub line_style: String,
    pub arrow_head: String,
    pub x1: f64,
    pub x2: f64,
    pub y: f64,
    pub is_self: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activation {
    pub actor_id: String,
    pub x: f64,
    pub top_y: f64,
    pub bottom_y: f64,
    pub width: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Divider {
    pub y: f64,
    pub label: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct PositionedBlock {
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub dividers: Vec<Divider>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PositionedNote {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub actors: Vec<String>,
    pub position: String,
}
static ACTOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(participant|actor)\s+(\S+?)(?:\s+as\s+(.+))?$").unwrap());
static NOTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^Note\s+(left of|right of|over)\s+([^:]+):\s*(.+)$").unwrap()
});
static BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(loop|alt|opt|par|critical|break|rect)(?:\s+(.*))?$").unwrap());
static DIVIDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(else|and)(?:\s+(.*))?$").unwrap());
static MESSAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\S+?)\s*(-->>|->>|--\)|-\)|--x|-x|-->|->)\s*([+-]?)(\S+?)\s*:\s*(.+)$").unwrap()
});
fn ensure(d: &mut Diagram, id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Empty participant ID".into());
    }
    if !d.actors.iter().any(|a| a.id == id) {
        if d.actors.len() >= 100 {
            return Err("At most 100 sequence participants are supported".into());
        }
        d.actors.push(Actor {
            id: id.into(),
            label: id.into(),
            kind: "participant".into(),
        });
    }
    Ok(())
}
fn parse(source: &str) -> Result<Diagram, String> {
    if source.len() > 65_536 {
        return Err("Diagram exceeds the 64 KiB input limit".into());
    }
    let mut lines = source
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim().starts_with("%%"));
    if !lines
        .next()
        .is_some_and(|(_, l)| l.trim().eq_ignore_ascii_case("sequenceDiagram"))
    {
        return Err("Expected sequenceDiagram header".into());
    }
    let mut d = Diagram::default();
    let mut stack: Vec<Block> = vec![];
    for (index, line) in lines {
        let line = line.trim();
        let result = (|| -> Result<(), String> {
            if let Some(c) = ACTOR.captures(line) {
                let id = &c[2];
                if !d.actors.iter().any(|a| a.id == id) {
                    ensure(&mut d, id)?;
                    let a = d.actors.last_mut().unwrap();
                    a.kind = c[1].into();
                    a.label = normalize_br_tags(c.get(3).map_or(id, |v| v.as_str().trim()));
                }
                return Ok(());
            }
            if let Some(c) = NOTE.captures(line) {
                let actors: Vec<_> = c[2].split(',').map(|s| s.trim().to_string()).collect();
                for id in &actors {
                    ensure(&mut d, id)?;
                }
                let position = match c[1].to_lowercase().as_str() {
                    "left of" => "left",
                    "right of" => "right",
                    _ => "over",
                };
                d.notes.push(Note {
                    actors,
                    text: normalize_br_tags(c[3].trim()),
                    position: position.into(),
                    after: d.messages.len() as isize - 1,
                });
                if d.notes.len() > 200 {
                    return Err("At most 200 sequence notes are supported".into());
                }
                return Ok(());
            }
            if let Some(c) = BLOCK.captures(line) {
                if !stack.is_empty() {
                    return Err("Nested sequence control blocks are not supported".into());
                }
                stack.push(Block {
                    kind: c[1].into(),
                    label: normalize_br_tags(c.get(2).map_or("", |v| v.as_str().trim())),
                    start: d.messages.len(),
                    end: 0,
                    dividers: vec![],
                });
                return Ok(());
            }
            if let Some(c) = DIVIDER.captures(line) {
                let b = stack.last_mut().ok_or("Divider outside a block")?;
                if (c[1].eq("else") && b.kind != "alt") || (c[1].eq("and") && b.kind != "par") {
                    return Err("else requires alt; and requires par".into());
                }
                b.dividers.push((
                    d.messages.len(),
                    normalize_br_tags(c.get(2).map_or("", |v| v.as_str().trim())),
                ));
                return Ok(());
            }
            if line == "end" {
                let mut b = stack.pop().ok_or("Unexpected block end")?;
                if d.messages.len() == b.start {
                    return Err("Sequence blocks must contain a message".into());
                }
                b.end = d.messages.len() - 1;
                d.blocks.push(b);
                return Ok(());
            }
            if let Some(c) = MESSAGE.captures(line) {
                let from = &c[1];
                let to = &c[4];
                let op = &c[2];
                if op.ends_with('x') {
                    return Err("Cross arrowheads are not yet supported".into());
                }
                ensure(&mut d, from)?;
                ensure(&mut d, to)?;
                if d.messages.len() >= 200 {
                    return Err("At most 200 sequence messages are supported".into());
                }
                d.messages.push(Message {
                    from: from.into(),
                    to: to.into(),
                    label: normalize_br_tags(c[5].trim()),
                    line: if op.starts_with("--") {
                        "dashed"
                    } else {
                        "solid"
                    }
                    .into(),
                    head: if op.contains(">>") { "filled" } else { "open" }.into(),
                    activate: &c[3] == "+",
                    deactivate: &c[3] == "-",
                });
                return Ok(());
            }
            Err(format!("Unsupported sequence syntax: {line}"))
        })();
        result.map_err(|e| format!("Line {}: {e}", index + 1))?;
    }
    if !stack.is_empty() {
        return Err("Unclosed sequence block".into());
    }
    if d.actors.is_empty() {
        return Err("Sequence diagram has no participants".into());
    }
    Ok(d)
}
fn positioned_note(
    n: &Note,
    y: f64,
    index: &BTreeMap<String, usize>,
    centers: &[f64],
    widths: &[f64],
) -> PositionedNote {
    let width = 60.0_f64.max(measure_text_width(&n.text, 11.0, 400) + 24.0);
    let height = 23.0 + n.text.matches('\n').count() as f64 * 14.3;
    let first = index[&n.actors[0]];
    let x = match n.position.as_str() {
        "left" => centers[first] - widths[first] / 2.0 - width - 10.0,
        "right" => centers[first] + widths[first] / 2.0 + 10.0,
        _ => (centers[first] + centers[index[n.actors.last().unwrap()]]) / 2.0 - width / 2.0,
    };
    PositionedNote {
        text: n.text.clone(),
        x,
        y,
        width,
        height,
        actors: n.actors.clone(),
        position: n.position.clone(),
    }
}
/// Parse and position a sequence diagram using the upstream timeline algorithm.
pub fn layout(source: &str, _options: &RenderOptions) -> Result<PositionedSequence, String> {
    let d = parse(source)?;
    let widths: Vec<_> = d
        .actors
        .iter()
        .map(|a| (measure_text_width(&a.label, 13.0, 500) + 32.0).max(80.0))
        .collect();
    let mut centers = vec![];
    let mut x = 30.0 + widths[0] / 2.0;
    for i in 0..d.actors.len() {
        if i > 0 {
            x += 140.0_f64.max((widths[i - 1] + widths[i]) / 2.0 + 40.0);
        }
        centers.push(x);
    }
    let index: BTreeMap<_, _> = d
        .actors
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id.clone(), i))
        .collect();
    let mut actors: Vec<_> = d
        .actors
        .iter()
        .enumerate()
        .map(|(i, a)| PositionedActor {
            id: a.id.clone(),
            label: a.label.clone(),
            kind: a.kind.clone(),
            x: centers[i],
            y: 30.0,
            width: widths[i],
            height: 40.0,
        })
        .collect();
    let mut extras: BTreeMap<usize, f64> = BTreeMap::new();
    for b in &d.blocks {
        extras
            .entry(b.start)
            .and_modify(|v| *v = v.max(28.0))
            .or_insert(28.0);
        for (idx, _) in &b.dividers {
            extras
                .entry(*idx)
                .and_modify(|v| *v = v.max(32.0))
                .or_insert(32.0);
        }
    }
    let mut notes = vec![];
    // Upstream actor labels extend below the header box; leave room before messages.
    let actor_clearance = d
        .actors
        .iter()
        .filter(|a| a.kind == "actor")
        .map(|a| 24.0 + a.label.matches('\n').count() as f64 * 16.9)
        .fold(0.0, f64::max);
    let mut y = 90.0 + actor_clearance;
    // Preserve initial notes; upstream silently drops notes before the first message.
    for n in d.notes.iter().filter(|n| n.after < 0) {
        let p = positioned_note(n, y, &index, &centers, &widths);
        y += p.height + 24.0;
        notes.push(p);
    }
    let mut messages = vec![];
    let mut stacks: BTreeMap<String, Vec<(f64, usize)>> = BTreeMap::new();
    let mut activations = vec![];
    for (i, m) in d.messages.iter().enumerate() {
        y += extras.get(&i).copied().unwrap_or(0.0);
        let is_self = m.from == m.to;
        messages.push(PositionedMessage {
            from: m.from.clone(),
            to: m.to.clone(),
            label: m.label.clone(),
            line_style: m.line.clone(),
            arrow_head: m.head.clone(),
            x1: centers[index[&m.from]],
            x2: centers[index[&m.to]],
            y,
            is_self,
        });
        if m.activate {
            let s = stacks.entry(m.to.clone()).or_default();
            s.push((y, s.len()));
        }
        if m.deactivate {
            if let Some((top, depth)) = stacks.get_mut(&m.from).and_then(Vec::pop) {
                activations.push(Activation {
                    actor_id: m.from.clone(),
                    x: centers[index[&m.from]] - 5.0 + depth as f64 * 4.0,
                    top_y: top,
                    bottom_y: y,
                    width: 10.0,
                });
            } else {
                return Err(format!("Cannot deactivate inactive participant {}", m.from));
            }
        }
        let mut note_y = y + if is_self { 38.0 } else { 8.0 };
        y += if is_self { 70.0 } else { 40.0 };
        let mut has_notes = false;
        for n in d.notes.iter().filter(|n| n.after == i as isize) {
            has_notes = true;
            let p = positioned_note(n, note_y, &index, &centers, &widths);
            note_y += p.height + 4.0;
            notes.push(p);
        }
        if has_notes {
            y = y.max(note_y + 20.0);
        }
    }
    for a in &d.actors {
        if let Some(s) = stacks.get(&a.id) {
            for (top, depth) in s {
                activations.push(Activation {
                    actor_id: a.id.clone(),
                    x: centers[index[&a.id]] - 5.0 + *depth as f64 * 4.0,
                    top_y: *top,
                    bottom_y: y - 20.0,
                    width: 10.0,
                });
            }
        }
    }
    let mut blocks = vec![];
    for b in &d.blocks {
        let involved: BTreeSet<_> = d.messages[b.start..=b.end]
            .iter()
            .flat_map(|m| [index[&m.from], index[&m.to]])
            .collect();
        let min = *involved.first().unwrap();
        let max = *involved.last().unwrap();
        let left = centers[min] - widths[min] / 2.0 - 10.0;
        let right = centers[max] + widths[max] / 2.0 + 10.0;
        let dividers = b
            .dividers
            .iter()
            .map(|(i, label)| {
                let msg = messages.get(*i);
                // Font estimates can miss horizontal overlap. Reserve vertical clearance
                // for every divider instead of relying on the upstream overlap heuristic.
                let offset = 44.0;
                Divider {
                    y: msg.map_or(y, |m| m.y) - offset,
                    label: label.clone(),
                }
            })
            .collect();
        let top = messages[b.start].y - 40.0;
        let bottom = messages[b.end].y + 20.0;
        blocks.push(PositionedBlock {
            kind: b.kind.clone(),
            label: b.label.clone(),
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
            dividers,
        });
    }
    let mut min_x = 30.0_f64;
    let mut max_x = 0.0_f64;
    for a in &actors {
        min_x = min_x.min(a.x - a.width / 2.0);
        max_x = max_x.max(a.x + a.width / 2.0);
    }
    for b in &blocks {
        min_x = min_x.min(b.x);
        max_x = max_x.max(b.x + b.width);
    }
    for n in &notes {
        min_x = min_x.min(n.x);
        max_x = max_x.max(n.x + n.width);
    }
    for m in &messages {
        if m.is_self {
            max_x = max_x.max(m.x1 + 38.0 + measure_text_width(&m.label, 11.0, 400) + 8.0);
        }
    }
    let shift = (30.0 - min_x).max(0.0);
    for a in &mut actors {
        a.x += shift;
    }
    for m in &mut messages {
        m.x1 += shift;
        m.x2 += shift;
    }
    for a in &mut activations {
        a.x += shift;
    }
    for b in &mut blocks {
        b.x += shift;
    }
    for n in &mut notes {
        n.x += shift;
    }
    for x in &mut centers {
        *x += shift;
    }
    let lifelines = d
        .actors
        .iter()
        .enumerate()
        .map(|(i, a)| Lifeline {
            actor_id: a.id.clone(),
            x: centers[i],
            top_y: 70.0,
            bottom_y: y,
        })
        .collect();
    Ok(PositionedSequence {
        width: (max_x + shift + 30.0).max(200.0),
        height: (y + 30.0).max(100.0),
        actors,
        lifelines,
        messages,
        activations,
        blocks,
        notes,
    })
}
/// Render a sequence diagram with native SVG colors and local fonts.
pub fn render(source: &str, options: &RenderOptions) -> Result<String, String> {
    let d = layout(source, options)?;
    render_positioned(&d, options)
}
/// Render already positioned geometry for reference comparison and native consumers.
pub fn render_positioned(
    d: &PositionedSequence,
    options: &RenderOptions,
) -> Result<String, String> {
    let p = theme::resolve(&options.colors)?;
    let muted = p.text_muted.clone();
    let mut out = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" font-family="{}, system-ui, sans-serif">"#,
        d.width,
        d.height,
        d.width,
        d.height,
        escape(&options.font)
    );
    if !options.transparent {
        write!(out, r#"<rect width="100%" height="100%" fill="{}"/>"#, p.bg).unwrap();
    }
    write!(out,r#"<defs><marker id="seq-arrow" markerWidth="8" markerHeight="5" refX="8" refY="2.5" orient="auto-start-reverse"><polygon points="0 0, 8 2.5, 0 5" fill="{}"/></marker><marker id="seq-arrow-open" markerWidth="8" markerHeight="5" refX="8" refY="2.5" orient="auto-start-reverse"><polyline points="0 0, 8 2.5, 0 5" fill="none" stroke="{}" stroke-width="1"/></marker></defs>"#,p.arrow,p.arrow).unwrap();
    // Block tabs must remain above lifelines and activation bars.
    for l in &d.lifelines {
        write!(out,r#"<line class="lifeline" data-actor="{}" x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="0.75" stroke-dasharray="6 4"/>"#,escape(&l.actor_id),l.x,l.top_y,l.x,l.bottom_y,p.line).unwrap();
    }
    for a in &d.activations {
        write!(out,r#"<rect class="activation" data-actor="{}" x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="0.75"/>"#,escape(&a.actor_id),a.x,a.top_y,a.width,a.bottom_y-a.top_y,p.node_fill,p.node_stroke).unwrap();
    }
    for b in &d.blocks {
        write!(out,r#"<g class="block" data-type="{}" data-label="{}"><rect x="{}" y="{}" width="{}" height="{}" rx="0" ry="0" fill="none" stroke="{}" stroke-width="1"/>"#,escape(&b.kind),escape(&b.label),b.x,b.y,b.width,b.height,p.node_stroke).unwrap();
        let label = if b.label.is_empty() {
            b.kind.clone()
        } else {
            format!("{} [{}]", b.kind, b.label)
        };
        let tab = measure_text_width(label.split('\n').next().unwrap(), 11.0, 600) + 16.0;
        write!(out,r#"<rect x="{}" y="{}" width="{tab}" height="18" fill="{}" stroke="{}" stroke-width="1"/>"#,b.x,b.y,p.group_header,p.node_stroke).unwrap();
        out.push_str(&text(
            &label,
            b.x + 6.0,
            b.y + 9.0,
            11.0,
            600,
            &p.secondary,
            false,
        ));
        for divider in &b.dividers {
            write!(out,r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="0.75" stroke-dasharray="6 4"/>"#,b.x,divider.y,b.x+b.width,divider.y,p.line).unwrap();
            if !divider.label.is_empty() {
                out.push_str(&text(
                    &format!("[{}]", divider.label),
                    b.x + 8.0,
                    divider.y + 14.0,
                    11.0,
                    400,
                    &muted,
                    false,
                ));
            }
        }
        out.push_str("</g>");
    }
    for m in &d.messages {
        let dash = if m.line_style == "dashed" {
            r#" stroke-dasharray="6 4""#
        } else {
            ""
        };
        let marker = if m.arrow_head == "filled" {
            "seq-arrow"
        } else {
            "seq-arrow-open"
        };
        write!(out,r#"<g class="message" data-from="{}" data-to="{}" data-label="{}" data-line-style="{}" data-arrow-head="{}" data-self="{}">"#,escape(&m.from),escape(&m.to),escape(&m.label),m.line_style,m.arrow_head,m.is_self).unwrap();
        if m.is_self {
            write!(out,r#"<polyline points="{},{} {},{} {},{} {},{}" fill="none" stroke="{}" stroke-width="1"{dash} marker-end="url(#{marker})"/>"#,m.x1,m.y,m.x1+30.0,m.y,m.x1+30.0,m.y+20.0,m.x2,m.y+20.0,p.line).unwrap();
            out.push_str(&text(
                &m.label,
                m.x1 + 38.0,
                m.y + 10.0,
                11.0,
                400,
                &muted,
                false,
            ));
        } else {
            write!(out,r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="1"{dash} marker-end="url(#{marker})"/>"#,m.x1,m.y,m.x2,m.y,p.line).unwrap();
            out.push_str(&text(
                &m.label,
                (m.x1 + m.x2) / 2.0,
                m.y - 10.0,
                11.0,
                400,
                &muted,
                true,
            ));
        }
        out.push_str("</g>");
    }
    for n in &d.notes {
        let (x, y, w, h) = (n.x, n.y, n.width, n.height);
        write!(out,r#"<g class="note" data-position="{}" data-actors="{}"><polygon points="{x},{y} {},{y} {},{} {},{} {x},{}" fill="{}" stroke="{}" stroke-width="0.75"/><polygon points="{},{y} {},{} {},{}" fill="{}" stroke="{}" stroke-width="0.75"/>"#,escape(&n.position),escape(&n.actors.join(",")),x+w-6.0,x+w,y+6.0,x+w,y+h,y+h,p.bg,p.node_stroke,x+w-6.0,x+w,y+6.0,x+w-6.0,y+6.0,p.inner_stroke,p.node_stroke).unwrap();
        out.push_str(&text(
            &n.text,
            x + w / 2.0,
            y + h / 2.0,
            11.0,
            400,
            &muted,
            true,
        ));
        out.push_str("</g>");
    }
    for a in &d.actors {
        write!(
            out,
            r#"<g class="actor" data-id="{}" data-label="{}" data-type="{}">"#,
            escape(&a.id),
            escape(&a.label),
            escape(&a.kind)
        )
        .unwrap();
        if a.kind == "actor" {
            let s = a.height / 24.0 * 0.9;
            let tx = a.x - 12.0 * s;
            let ty = a.y + (a.height - 24.0 * s) / 2.0;
            write!(out,r#"<g transform="translate({tx},{ty}) scale({s})" fill="none" stroke="{}" stroke-width="{}"><path d="M21 12C21 16.9706 16.9706 21 12 21C7.02944 21 3 16.9706 3 12C3 7.02944 7.02944 3 12 3C16.9706 3 21 7.02944 21 12Z"/><path d="M15 10C15 11.6569 13.6569 13 12 13C10.3431 13 9 11.6569 9 10C9 8.34315 10.3431 7 12 7C13.6569 7 15 8.34315 15 10Z"/><path d="M5.62842 18.3563C7.08963 17.0398 9.39997 16 12 16C14.6 16 16.9104 17.0398 18.3716 18.3563"/></g>"#,p.line,1.0/s).unwrap();
            out.push_str(&text(
                &a.label,
                a.x,
                a.y + a.height + 14.0,
                13.0,
                500,
                &p.text,
                true,
            ));
        } else {
            write!(out,r#"<rect x="{}" y="{}" width="{}" height="{}" rx="4" ry="4" fill="{}" stroke="{}" stroke-width="1"/>"#,a.x-a.width/2.0,a.y,a.width,a.height,p.node_fill,p.node_stroke).unwrap();
            out.push_str(&text(
                &a.label,
                a.x,
                a.y + a.height / 2.0,
                13.0,
                500,
                &p.text,
                true,
            ));
        }
        out.push_str("</g>");
    }
    out.push_str("</svg>");
    Ok(out)
}
