//! Native port of Beautiful Mermaid 1.1.3's flowchart ELK adapter.
//! Copyright (c) 2026 Craft Docs. Distributed under the MIT license.
use crate::{metrics::measure_multiline_text, types::*};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn direction(dir: &str) -> &'static str {
    match dir {
        "LR" => "RIGHT",
        "RL" => "LEFT",
        "BT" => "UP",
        _ => "DOWN",
    }
}

pub fn estimate_node_size(label: &str, shape: &str) -> (f64, f64) {
    let metrics = measure_multiline_text(label, 13.0, 500);
    let (mut w, mut h) = (metrics.width + 40.0, metrics.height + 20.0);
    match shape {
        "diamond" => {
            w = w.max(h) + 24.0;
            h = w;
        }
        "circle" | "doublecircle" => {
            w = (w * w + h * h).sqrt().ceil() + 8.0;
            if shape == "doublecircle" {
                w += 12.0;
            }
            h = w;
        }
        "hexagon" | "trapezoid" | "trapezoid-alt" => w += 20.0,
        "asymmetric" => w += 12.0,
        "cylinder" => h += 14.0,
        "state-start" | "state-end" => return (28.0, 28.0),
        _ => {}
    }
    (w.max(60.0), h.max(36.0))
}
fn leaf(node: &Node) -> Value {
    let (w, h) = estimate_node_size(&node.label, &node.shape);
    json!({"id":node.id,"width":w,"height":h,"labels":[{"text":node.label}]})
}
fn edge_json(i: usize, edge: &Edge, source: &str, target: &str) -> Value {
    let mut value = json!({"id":format!("e{i}"),"sources":[source],"targets":[target]});
    if let Some(label) = edge.label.as_deref().filter(|l| !l.is_empty()) {
        let m = measure_multiline_text(label, 11.0, 400);
        value["labels"] = json!([{"text":label,"width":m.width+8.0,"height":m.height+6.0,
            "layoutOptions":{"elk.edgeLabels.inline":"true","elk.edgeLabels.placement":"CENTER"}}]);
    }
    value
}
fn collect<'a>(
    groups: &'a [Subgraph],
    owners: &mut BTreeMap<&'a str, &'a str>,
    all: &mut BTreeMap<&'a str, &'a Subgraph>,
) {
    for group in groups {
        all.insert(&group.id, group);
        for id in &group.node_ids {
            owners.insert(id, &group.id);
        }
        collect(&group.children, owners, all);
    }
}
#[derive(Clone)]
struct Port {
    id: String,
    index: usize,
    incoming: bool,
    node: String,
}
fn compound(
    sg: &Subgraph,
    graph: &Graph,
    opts: &RenderOptions,
    edges: &BTreeMap<String, Vec<usize>>,
    ports: &BTreeMap<String, Vec<Port>>,
) -> Value {
    let mut options = json!({"elk.algorithm":"layered","elk.padding":"[top=44,left=16,bottom=16,right=16]",
        "elk.edgeRouting":"ORTHOGONAL","elk.contentAlignment":"H_CENTER V_CENTER",
        "elk.spacing.edgeEdge":"12","elk.layered.spacing.edgeEdgeBetweenLayers":"12",
        "elk.layered.spacing.edgeNodeBetweenLayers":"12","elk.layered.nodePlacement.bk.fixedAlignment":"BALANCED",
        "elk.layered.spacing.nodeNodeBetweenLayers":opts.layer_spacing.to_string(),"elk.spacing.nodeNode":opts.node_spacing.to_string()});
    if let Some(dir) = &sg.direction {
        options["elk.direction"] = json!(direction(dir));
    }
    let mut children = Vec::new();
    for id in &sg.node_ids {
        if let Some(node) = graph.nodes.iter().find(|n| &n.id == id) {
            children.push(leaf(node));
        }
    }
    for child in &sg.children {
        children.push(compound(child, graph, opts, edges, ports));
    }
    let mut inner = Vec::new();
    if let Some(indices) = edges.get(&sg.id) {
        for &i in indices {
            let e = &graph.edges[i];
            inner.push(edge_json(i, e, &e.source, &e.target));
        }
    }
    if let Some(list) = ports.get(&sg.id) {
        for p in list {
            let (source, target) = if p.incoming {
                (&p.id, &p.node)
            } else {
                (&p.node, &p.id)
            };
            inner.push(
                json!({"id":format!("e{}_internal",p.index),"sources":[source],"targets":[target]}),
            );
        }
    }
    let mut value = json!({"id":sg.id,"layoutOptions":options,"children":children,"edges":inner});
    if !sg.label.is_empty() {
        value["labels"] = json!([{"text":sg.label}]);
    }
    if let Some(list) = ports.get(&sg.id) {
        value["ports"] = json!(list.iter().map(|p| json!({"id":p.id})).collect::<Vec<_>>());
    }
    value
}

/// Convert the parsed graph into the upstream-compatible ELK JSON input.
pub fn build_elk_graph(graph: &Graph, opts: &RenderOptions) -> Result<Value, String> {
    let mut owners = BTreeMap::new();
    let mut groups = BTreeMap::new();
    collect(&graph.subgraphs, &mut owners, &mut groups);
    fn nested_override(groups: &[Subgraph]) -> bool {
        groups
            .iter()
            .any(|g| g.direction.is_some() || nested_override(&g.children))
    }
    if graph.subgraphs.iter().any(|g| nested_override(&g.children)) {
        return Err("Direction overrides on nested subgraphs are not yet supported; use a top-level subgraph direction".into());
    }
    let separate = graph.subgraphs.iter().any(|sg| sg.direction.is_some());
    let mut edges: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut cross = Vec::new();
    let mut root = Vec::new();
    for (i, e) in graph.edges.iter().enumerate() {
        let source = owners.get(e.source.as_str()).copied();
        let target = owners.get(e.target.as_str()).copied();
        match (source, target) {
            (Some(a), Some(b)) if a == b => edges.entry(a.into()).or_default().push(i),
            (None, None) => root.push(i),
            _ => cross.push((i, source, target)),
        }
    }
    let mut ports: BTreeMap<String, Vec<Port>> = BTreeMap::new();
    if separate {
        for &(i, source, target) in &cross {
            let e = &graph.edges[i];
            if let Some(sg) = source {
                ports.entry(sg.into()).or_default().push(Port {
                    id: format!("{sg}_out_{i}"),
                    index: i,
                    incoming: false,
                    node: e.source.clone(),
                });
            }
            if let Some(sg) = target {
                ports.entry(sg.into()).or_default().push(Port {
                    id: format!("{sg}_in_{i}"),
                    index: i,
                    incoming: true,
                    node: e.target.clone(),
                });
            }
        }
    }
    let mut children: Vec<_> = graph
        .nodes
        .iter()
        .filter(|n| !owners.contains_key(n.id.as_str()) && !groups.contains_key(n.id.as_str()))
        .map(leaf)
        .collect();
    for sg in &graph.subgraphs {
        children.push(compound(sg, graph, opts, &edges, &ports));
    }
    let mut root_edges: Vec<_> = root
        .iter()
        .map(|&i| {
            let e = &graph.edges[i];
            edge_json(i, e, &e.source, &e.target)
        })
        .collect();
    for (i, source, target) in cross {
        let e = &graph.edges[i];
        let src = if separate {
            source
                .map(|s| format!("{s}_out_{i}"))
                .unwrap_or_else(|| e.source.clone())
        } else {
            e.source.clone()
        };
        let dst = if separate {
            target
                .map(|s| format!("{s}_in_{i}"))
                .unwrap_or_else(|| e.target.clone())
        } else {
            e.target.clone()
        };
        root_edges.push(edge_json(i, e, &src, &dst));
    }
    Ok(
        json!({"id":"root","children":children,"edges":root_edges,"layoutOptions":{
            "elk.algorithm":"layered","elk.direction":direction(&graph.direction),
            "elk.spacing.nodeNode":opts.node_spacing.to_string(),"elk.layered.spacing.nodeNodeBetweenLayers":opts.layer_spacing.to_string(),
            "elk.spacing.edgeEdge":"12","elk.layered.spacing.edgeEdgeBetweenLayers":"12","elk.layered.spacing.edgeNodeBetweenLayers":"12",
            "elk.padding":format!("[top={},left={},bottom={},right={}]",opts.padding,opts.padding,opts.padding,opts.padding),
            "elk.edgeRouting":"ORTHOGONAL","elk.layered.nodePlacement.bk.fixedAlignment":"BALANCED",
            "elk.contentAlignment":"H_CENTER V_CENTER","elk.layered.thoroughness":"3",
            "elk.layered.highDegreeNodes.treatment":"true","elk.layered.highDegreeNodes.threshold":"8",
            "elk.layered.compaction.postCompaction.strategy":"LEFT_RIGHT_CONSTRAINT_LOCKING",
            "elk.layered.considerModelOrder.strategy":"NODES_AND_EDGES","elk.layered.wrapping.strategy":"OFF",
            "elk.hierarchyHandling":if separate {"SEPARATE"} else {"INCLUDE_CHILDREN"}
        }}),
    )
}
fn n(v: &Value, k: &str) -> f64 {
    v[k].as_f64().unwrap_or(0.0)
}
fn array<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v[k].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn style(graph: &Graph, id: &str) -> Style {
    let mut result = graph
        .class_assignments
        .get(id)
        .and_then(|c| graph.class_defs.get(c))
        .cloned()
        .unwrap_or_default();
    if let Some(explicit) = graph.node_styles.get(id) {
        result.extend(explicit.clone());
    }
    result
}
fn extract_nodes(
    v: &Value,
    graph: &Graph,
    all: &BTreeMap<&str, &Subgraph>,
    nodes: &mut Vec<PositionedNode>,
    groups: &mut Vec<PositionedGroup>,
    ox: f64,
    oy: f64,
) {
    for child in array(v, "children") {
        let id = child["id"].as_str().unwrap_or("");
        let (x, y) = (n(child, "x") + ox, n(child, "y") + oy);
        if let Some(sg) = all.get(id) {
            let mut nested = Vec::new();
            extract_nodes(child, graph, all, nodes, &mut nested, x, y);
            groups.push(PositionedGroup {
                id: id.into(),
                label: sg.label.clone(),
                x,
                y,
                width: n(child, "width"),
                height: n(child, "height"),
                children: nested,
            });
        } else if let Some(node) = graph.nodes.iter().find(|node| node.id == id) {
            nodes.push(PositionedNode {
                id: id.into(),
                label: node.label.clone(),
                shape: node.shape.clone(),
                x,
                y,
                width: n(child, "width"),
                height: n(child, "height"),
                inline_style: style(graph, id),
            });
            extract_nodes(child, graph, all, nodes, groups, x, y);
        }
    }
}
#[derive(Default, Clone)]
struct Segment {
    points: Vec<Point>,
    label: Option<Point>,
}
#[derive(Default)]
struct Segments {
    index: usize,
    external: Option<Segment>,
    incoming: Option<Segment>,
    outgoing: Option<Segment>,
}
fn segments(
    v: &Value,
    out: &mut Vec<Segments>,
    ox: f64,
    oy: f64,
    offsets: &BTreeMap<String, Point>,
) -> Result<(), String> {
    for e in array(v, "edges") {
        let id = e["id"]
            .as_str()
            .ok_or("ELK returned an edge without an ID")?;
        let index = id
            .trim_start_matches('e')
            .split('_')
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or("ELK returned an invalid edge ID")?;
        let internal = id.ends_with("_internal");
        // ELK may store an edge at the root while its coordinates are local to
        // the explicit container. Upstream 1.1.3 ignores this metadata.
        let origin = e["container"]
            .as_str()
            .and_then(|id| offsets.get(id))
            .copied()
            .unwrap_or(Point { x: ox, y: oy });
        let (edge_x, edge_y) = (origin.x, origin.y);
        let mut segment = Segment::default();
        // One section per edge is the contract used by Beautiful Mermaid.
        let sections = array(e, "sections");
        if sections.len() > 1 {
            return Err(format!(
                "Multiple ELK sections for edge {id} are not yet supported"
            ));
        }
        if let Some(section) = sections.first() {
            let point = |p: &Value| Point {
                x: n(p, "x") + edge_x,
                y: n(p, "y") + edge_y,
            };
            segment.points.push(point(&section["startPoint"]));
            for bend in array(section, "bendPoints") {
                segment.points.push(point(bend));
            }
            segment.points.push(point(&section["endPoint"]));
        }
        if let Some(label) = array(e, "labels")
            .first()
            .filter(|l| l["x"].is_number() && l["y"].is_number())
        {
            segment.label = Some(Point {
                x: n(label, "x") + n(label, "width") / 2.0 + edge_x,
                y: n(label, "y") + n(label, "height") / 2.0 + edge_y,
            });
        }
        let pos = out
            .iter()
            .position(|s| s.index == index)
            .unwrap_or_else(|| {
                out.push(Segments {
                    index,
                    ..Default::default()
                });
                out.len() - 1
            });
        if internal {
            let source = e["sources"][0].as_str().unwrap_or("");
            if source.contains("_in_") || source.contains("_out_") {
                out[pos].incoming = Some(segment);
            } else {
                out[pos].outgoing = Some(segment);
            }
        } else {
            out[pos].external = Some(segment);
        }
    }
    for child in array(v, "children") {
        segments(child, out, ox + n(child, "x"), oy + n(child, "y"), offsets)?;
    }
    Ok(())
}
fn collect_offsets(value: &Value, x: f64, y: f64, offsets: &mut BTreeMap<String, Point>) {
    if let Some(id) = value["id"].as_str() {
        offsets.insert(id.into(), Point { x, y });
    }
    for child in array(value, "children") {
        collect_offsets(child, x + n(child, "x"), y + n(child, "y"), offsets);
    }
}
fn midpoint(points: &[Point]) -> Point {
    let length = |a: Point, b: Point| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
    let mut remaining = points.windows(2).map(|p| length(p[0], p[1])).sum::<f64>() / 2.0;
    for p in points.windows(2) {
        let len = length(p[0], p[1]);
        if remaining <= len && len > 0.0 {
            let t = remaining / len;
            return Point {
                x: p[0].x + t * (p[1].x - p[0].x),
                y: p[0].y + t * (p[1].y - p[0].y),
            };
        }
        remaining -= len;
    }
    points.last().copied().unwrap_or(Point { x: 0.0, y: 0.0 })
}
fn bounds(groups: &[PositionedGroup], out: &mut Vec<(f64, f64)>) {
    for g in groups {
        out.push((g.x, g.x + g.width));
        bounds(&g.children, out);
    }
}
fn orthogonal(points: Vec<Point>, margins: Option<(f64, f64)>, index: usize) -> (Vec<Point>, bool) {
    if !points
        .windows(2)
        .any(|p| (p[0].x - p[1].x).abs() > 1.0 && (p[0].y - p[1].y).abs() > 1.0)
    {
        return (points, false);
    }
    let mut result = vec![points[0]];
    for curr in points.into_iter().skip(1) {
        let prev = *result.last().unwrap();
        if (curr.x - prev.x).abs() > 1.0 && (curr.y - prev.y).abs() > 1.0 {
            if let Some((left, right)) = margins {
                let offset = (index / 2) as f64 * 12.0;
                let x = if index.is_multiple_of(2) {
                    right + offset
                } else {
                    left - offset
                };
                result.push(Point { x, y: prev.y });
                result.push(Point { x, y: curr.y });
            } else {
                let y = (prev.y + curr.y) / 2.0;
                result.push(Point { x: prev.x, y });
                result.push(Point { x: curr.x, y });
            }
        }
        result.push(curr);
    }
    (result, true)
}
fn align(nodes: &mut [PositionedNode], edges: &mut [PositionedEdge], dir: &str) {
    if nodes.is_empty() {
        return;
    }
    let horizontal = dir == "LR" || dir == "RL";
    let mut sorted: Vec<usize> = (0..nodes.len()).collect();
    let coord = |n: &PositionedNode| if horizontal { n.x } else { n.y };
    sorted.sort_by(|&a, &b| coord(&nodes[a]).total_cmp(&coord(&nodes[b])));
    let connected: BTreeSet<_> = edges
        .iter()
        .flat_map(|e| {
            [
                (e.source.clone(), e.target.clone()),
                (e.target.clone(), e.source.clone()),
            ]
        })
        .collect();
    let mut layers = vec![vec![sorted[0]]];
    for pair in sorted.windows(2) {
        let i = pair[1];
        let layer = layers.last_mut().unwrap();
        if coord(&nodes[i]) - coord(&nodes[pair[0]]) <= 48.0 * 0.6
            && !layer
                .iter()
                .any(|&j| connected.contains(&(nodes[j].id.clone(), nodes[i].id.clone())))
        {
            layer.push(i);
        } else {
            layers.push(vec![i]);
        }
    }
    let mut deltas = BTreeMap::new();
    for layer in layers {
        let min = layer
            .iter()
            .map(|&i| coord(&nodes[i]))
            .fold(f64::INFINITY, f64::min);
        let max = layer
            .iter()
            .map(|&i| coord(&nodes[i]))
            .fold(f64::NEG_INFINITY, f64::max);
        if max - min <= 1.0 {
            continue;
        }
        let target = (min + max) / 2.0;
        for i in layer {
            let delta = target - coord(&nodes[i]);
            if delta.abs() > 0.5 {
                if horizontal {
                    nodes[i].x = target;
                } else {
                    nodes[i].y = target;
                }
                deltas.insert(nodes[i].id.clone(), delta);
            }
        }
    }
    for e in edges {
        // Native ELK and JSON round trips can differ by a few ULPs. Treat
        // geometrically collinear endpoint runs as equal when shifting layers.
        if e.points.len() < 2 {
            continue;
        }
        if let Some(&delta) = deltas.get(&e.source) {
            if horizontal {
                e.points[0].x += delta;
                if (e.points[1].x - (e.points[0].x - delta)).abs() < 1e-6 {
                    e.points[1].x += delta;
                }
            } else {
                e.points[0].y += delta;
                if (e.points[1].y - (e.points[0].y - delta)).abs() < 1e-6 {
                    e.points[1].y += delta;
                }
            }
        }
        if let Some(&delta) = deltas.get(&e.target) {
            let i = e.points.len() - 1;
            if horizontal {
                e.points[i].x += delta;
                if (e.points[i - 1].x - (e.points[i].x - delta)).abs() < 1e-6 {
                    e.points[i - 1].x += delta;
                }
            } else {
                e.points[i].y += delta;
                if (e.points[i - 1].y - (e.points[i].y - delta)).abs() < 1e-6 {
                    e.points[i - 1].y += delta;
                }
            }
        }
    }
}
fn containing<'a>(
    x: f64,
    y: f64,
    groups: &'a [PositionedGroup],
    out: &mut Vec<&'a PositionedGroup>,
) {
    for g in groups {
        if x >= g.x && x <= g.x + g.width && y >= g.y && y <= g.y + g.height {
            out.push(g);
            containing(x, y, &g.children, out);
        }
    }
}
fn adjust(j: f64, x: f64, y: f64, groups: &[PositionedGroup], dir: &str) -> f64 {
    let mut own = Vec::new();
    containing(x, y, groups, &mut own);
    let horizontal = dir == "LR" || dir == "RL";
    let mut probe = Vec::new();
    containing(
        if horizontal { j } else { x },
        if horizontal { y } else { j },
        groups,
        &mut probe,
    );
    if let Some(g) = probe
        .into_iter()
        .find(|g| !own.iter().any(|n| n.id == g.id))
    {
        match dir {
            "LR" => g.x - 12.0,
            "RL" => g.x + g.width + 12.0,
            "BT" => g.y + g.height + 12.0,
            _ => g.y - 12.0,
        }
    } else {
        j
    }
}
fn forward(s: &PositionedNode, t: &PositionedNode, dir: &str) -> bool {
    match dir {
        "LR" => t.x > s.x + s.width,
        "RL" => t.x + t.width < s.x,
        "BT" => t.y + t.height < s.y,
        _ => t.y > s.y + s.height,
    }
}
fn bundle(
    edges: &mut [PositionedEdge],
    nodes: &[PositionedNode],
    groups: &[PositionedGroup],
    dir: &str,
) {
    let mut processed = BTreeSet::new();
    // Preserve original edge encounter order, matching JavaScript Map.
    for fan_in in [false, true] {
        let mut keys = Vec::new();
        for (i, e) in edges.iter().enumerate() {
            if e.source == e.target || (fan_in && processed.contains(&i)) {
                continue;
            }
            let key = if fan_in { &e.target } else { &e.source };
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        for key in keys {
            let group: Vec<_> = edges
                .iter()
                .enumerate()
                .filter(|(i, e)| {
                    e.source != e.target
                        && (!fan_in || !processed.contains(i))
                        && if fan_in {
                            e.target == key
                        } else {
                            e.source == key
                        }
                })
                .map(|(i, _)| i)
                .collect();
            if group.len() < 2
                || group.iter().any(|&i| {
                    edges[i].label.as_ref().is_some_and(|l| !l.is_empty())
                        || edges[i].style != edges[group[0]].style
                })
            {
                continue;
            }
            let Some(fixed) = nodes.iter().find(|n| n.id == key) else {
                continue;
            };
            let targets: Vec<_> = group
                .into_iter()
                .filter_map(|i| {
                    let e = &edges[i];
                    let other = nodes.iter().find(|n| {
                        n.id == if fan_in {
                            e.source.as_str()
                        } else {
                            e.target.as_str()
                        }
                    })?;
                    let (s, t) = if fan_in {
                        (other, fixed)
                    } else {
                        (fixed, other)
                    };
                    forward(s, t, dir).then_some((i, other))
                })
                .collect();
            if targets.len() < 2 {
                continue;
            }
            let horizontal = dir == "LR" || dir == "RL";
            let reverse = dir == "RL" || dir == "BT";
            let main = |n: &PositionedNode| if horizontal { n.x } else { n.y };
            let size = |n: &PositionedNode| if horizontal { n.width } else { n.height };
            let fixed_main = main(fixed) + if fan_in != reverse { 0.0 } else { size(fixed) };
            let others: Vec<_> = targets
                .iter()
                .map(|(_, n)| main(n) + if fan_in != reverse { size(n) } else { 0.0 })
                .collect();
            let nearest = if fan_in != reverse {
                others.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            } else {
                others.iter().copied().fold(f64::INFINITY, f64::min)
            };
            let junction = adjust(
                (fixed_main + nearest) / 2.0,
                fixed.x + fixed.width / 2.0,
                fixed.y + fixed.height / 2.0,
                groups,
                dir,
            );
            for (i, other) in targets {
                let (source, target) = if fan_in {
                    (other, fixed)
                } else {
                    (fixed, other)
                };
                if horizontal {
                    let sx = source.x + if reverse { 0.0 } else { source.width };
                    let sy = source.y + source.height / 2.0;
                    let tx = target.x + if reverse { target.width } else { 0.0 };
                    let ty = target.y + target.height / 2.0;
                    edges[i].points = vec![
                        Point { x: sx, y: sy },
                        Point { x: junction, y: sy },
                        Point { x: junction, y: ty },
                        Point { x: tx, y: ty },
                    ];
                } else {
                    let sx = source.x + source.width / 2.0;
                    let sy = source.y + if reverse { 0.0 } else { source.height };
                    let tx = target.x + target.width / 2.0;
                    let ty = target.y + if reverse { target.height } else { 0.0 };
                    edges[i].points = vec![
                        Point { x: sx, y: sy },
                        Point { x: sx, y: junction },
                        Point { x: tx, y: junction },
                        Point { x: tx, y: ty },
                    ];
                }
                if !fan_in {
                    processed.insert(i);
                }
            }
        }
    }
}
fn clip(points: &mut [Point], node: &PositionedNode, start: bool) {
    if points.len() < 2 || node.shape != "diamond" {
        return;
    }
    let i = if start { 0 } else { points.len() - 1 };
    let adjacent = points[if start { 1 } else { i - 1 }];
    let p = points[i];
    let (cx, cy) = (node.x + node.width / 2.0, node.y + node.height / 2.0);
    let top = Point { x: cx, y: node.y };
    let right = Point {
        x: node.x + node.width,
        y: cy,
    };
    let bottom = Point {
        x: cx,
        y: node.y + node.height,
    };
    let left = Point { x: node.x, y: cy };
    let dx = p.x - adjacent.x;
    let dy = p.y - adjacent.y;
    let vertical = dx.abs() < dy.abs();
    let (a, b, fallback) = if vertical {
        if dy > 0.0 {
            if p.x <= cx {
                (left, top, top)
            } else {
                (top, right, top)
            }
        } else if p.x <= cx {
            (bottom, left, bottom)
        } else {
            (right, bottom, bottom)
        }
    } else if dx > 0.0 {
        if p.y <= cy {
            (top, left, left)
        } else {
            (left, bottom, left)
        }
    } else if p.y <= cy {
        (top, right, right)
    } else {
        (right, bottom, right)
    };
    let delta = if vertical { b.x - a.x } else { b.y - a.y };
    let t = if vertical {
        (p.x - a.x) / delta
    } else {
        (p.y - a.y) / delta
    };
    points[i] = if delta.abs() < 0.001 || !(0.0..=1.0).contains(&t) {
        fallback
    } else {
        Point {
            x: a.x + t * (b.x - a.x),
            y: a.y + t * (b.y - a.y),
        }
    };
}

/// Convert a native ELK result into positioned primitives and upstream postprocessing.
pub fn extract_layout(result: &Value, graph: &Graph) -> Result<PositionedGraph, String> {
    let mut owners = BTreeMap::new();
    let mut all = BTreeMap::new();
    collect(&graph.subgraphs, &mut owners, &mut all);
    let mut nodes = Vec::new();
    let mut groups = Vec::new();
    extract_nodes(result, graph, &all, &mut nodes, &mut groups, 0.0, 0.0);
    let mut boxes = Vec::new();
    bounds(&groups, &mut boxes);
    let margins = (!boxes.is_empty()).then(|| {
        (
            boxes.iter().map(|p| p.0).fold(f64::INFINITY, f64::min) - 20.0,
            boxes.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max) + 20.0,
        )
    });
    let mut parts = Vec::new();
    let mut offsets = BTreeMap::new();
    collect_offsets(result, 0.0, 0.0, &mut offsets);
    segments(result, &mut parts, 0.0, 0.0, &offsets)?;
    let mut edges = Vec::new();
    let mut margin_index = 0;
    for part in parts {
        let original = graph
            .edges
            .get(part.index)
            .ok_or("ELK returned an unknown edge")?;
        let mut points = Vec::new();
        for segment in [&part.outgoing, &part.external, &part.incoming]
            .into_iter()
            .flatten()
        {
            points.extend(
                segment
                    .points
                    .iter()
                    .skip(usize::from(!points.is_empty()))
                    .copied(),
            );
        }
        if points.len() < 2 {
            return Err(format!("ELK returned no route for edge {}", part.index));
        }
        let has_label = original.label.as_ref().is_some_and(|l| !l.is_empty());
        let mut label_position = if has_label {
            Some(
                part.external
                    .as_ref()
                    .and_then(|s| s.label)
                    .unwrap_or_else(|| midpoint(&points)),
            )
        } else {
            None
        };
        let (points, changed) = orthogonal(points, margins, margin_index);
        if changed {
            margin_index += 1;
            if has_label {
                label_position = Some(midpoint(&points));
            }
        }
        let mut inline_style = graph
            .link_styles
            .get("default")
            .cloned()
            .unwrap_or_default();
        if let Some(style) = graph.link_styles.get(&part.index.to_string()) {
            inline_style.extend(style.clone());
        }
        edges.push(PositionedEdge {
            source: original.source.clone(),
            target: original.target.clone(),
            label: original.label.clone(),
            style: original.style.clone(),
            has_arrow_start: original.has_arrow_start,
            has_arrow_end: original.has_arrow_end,
            points,
            label_position,
            inline_style,
        });
    }
    if edges.len() != graph.edges.len() {
        return Err("ELK did not return every requested edge".into());
    }
    align(&mut nodes, &mut edges, &graph.direction);
    bundle(&mut edges, &nodes, &groups, &graph.direction);
    for edge in &mut edges {
        if let Some(node) = nodes.iter().find(|n| n.id == edge.source) {
            clip(&mut edge.points, node, true);
        }
        if let Some(node) = nodes.iter().find(|n| n.id == edge.target) {
            clip(&mut edge.points, node, false);
        }
    }
    let (mut width, mut height) = (
        result["width"].as_f64().unwrap_or(800.0),
        result["height"].as_f64().unwrap_or(600.0),
    );
    for edge in &edges {
        for p in &edge.points {
            width = width.max(p.x + 48.0);
            height = height.max(p.y + 48.0);
        }
        if let Some(p) = edge.label_position {
            width = width.max(p.x + 100.0);
            height = height.max(p.y + 60.0);
        }
    }
    Ok(PositionedGraph {
        width,
        height,
        nodes,
        edges,
        groups,
    })
}
/// Lay out a flowchart with native ELK; no JavaScript runtime is used.
pub fn layout_graph(graph: &Graph, opts: &RenderOptions) -> Result<PositionedGraph, String> {
    let input = build_elk_graph(graph, opts)?;
    let output = elk_layout::layout_json(&input.to_string(), "{}")?;
    let result: Value = serde_json::from_str(&output).map_err(|e| e.to_string())?;
    extract_layout(&result, graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_alignment_preserves_collinear_bends_after_roundoff() {
        let mut graph: PositionedGraph = serde_json::from_value(json!({
            "width":100,"height":100,"groups":[],
            "nodes":[
                {"id":"A","label":"A","shape":"rectangle","x":0,"y":0,"width":10,"height":10},
                {"id":"B","label":"B","shape":"rectangle","x":6,"y":60,"width":10,"height":10}
            ],
            "edges":[{"source":"Outside","target":"B","label":null,"style":"solid",
                "hasArrowStart":false,"hasArrowEnd":true,"labelPosition":null,
                "points":[{"x":-20,"y":20},{"x":6.000000000001,"y":20},{"x":6,"y":60}]}]
        }))
        .unwrap();
        align(&mut graph.nodes, &mut graph.edges, "LR");
        let points = &graph.edges[0].points;
        assert!((points[1].x - 3.0).abs() < 1e-6);
        assert!((points[2].x - 3.0).abs() < 1e-6);
    }
}
