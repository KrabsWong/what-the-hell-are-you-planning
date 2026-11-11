// Flowchart parser ported from Beautiful Mermaid 1.1.3 (MIT, Craft Docs).
// See licenses/Beautiful-Mermaid-MIT.txt. Invalid input is rejected rather than skipped.
use crate::{metrics::normalize_br_tags, types::*};
use regex::Regex;
use std::{collections::BTreeMap, sync::LazyLock};

static HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:graph|flowchart)\s+(TD|TB|LR|BT|RL)\s*$").unwrap());
static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]+").unwrap());
static CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^:::([A-Za-z0-9_][A-Za-z0-9_-]*)").unwrap());
static ARROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(<)?(-->|-\.->|==>|---|-\.-|===)(?:\|([^|]*)\|)?").unwrap());
static TEXT_ARROW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(<)?(--|-\.|==)\s+(.+?)\s+(-->|---|\.->|-\.-|==>|===)").unwrap()
});
static SUBGRAPH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z0-9_-]+)\s*\[(.+)\]$").unwrap());
static SHAPES: LazyLock<Vec<(Regex, &str)>> = LazyLock::new(|| {
    [
        (r"^([\w-]+)\(\(\((.+?)\)\)\)", "doublecircle"),
        (r"^([\w-]+)\(\[(.+?)\]\)", "stadium"),
        (r"^([\w-]+)\(\((.+?)\)\)", "circle"),
        (r"^([\w-]+)\[\[(.+?)\]\]", "subroutine"),
        (r"^([\w-]+)\[\((.+?)\)\]", "cylinder"),
        (r"^([\w-]+)\[/(.+?)\\\]", "trapezoid"),
        (r"^([\w-]+)\[\\(.+?)/\]", "trapezoid-alt"),
        (r"^([\w-]+)>(.+?)\]", "asymmetric"),
        (r"^([\w-]+)\{\{(.+?)\}\}", "hexagon"),
        (r"^([\w-]+)\[(.+?)\]", "rectangle"),
        (r"^([\w-]+)\((.+?)\)", "rounded"),
        (r"^([\w-]+)\{(.+?)\}", "diamond"),
    ]
    .into_iter()
    .map(|(r, s)| (Regex::new(r).unwrap(), s))
    .collect()
});

fn style(text: &str) -> Result<Style, String> {
    text.trim_end_matches(';')
        .split(',')
        .map(|pair| {
            let (k, v) = pair
                .split_once(':')
                .ok_or("Expected style property:value")?;
            if k.trim().is_empty() || v.trim().is_empty() {
                return Err("Empty style property or value".into());
            }
            Ok((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}
fn register(node: Node, graph: &mut Graph, stack: &mut [Subgraph]) {
    let id = node.id.clone();
    if !graph.nodes.iter().any(|n| n.id == id) {
        graph.nodes.push(node);
    }
    if let Some(group) = stack.last_mut()
        && !group.node_ids.contains(&id)
    {
        group.node_ids.push(id);
    }
}
fn node<'a>(
    text: &'a str,
    graph: &mut Graph,
    stack: &mut [Subgraph],
) -> Result<(String, &'a str), String> {
    let mut parsed = None;
    for (pattern, shape) in SHAPES.iter() {
        if let Some(c) = pattern.captures(text) {
            let id = c[1].to_owned();
            register(
                Node {
                    id: id.clone(),
                    label: normalize_br_tags(&c[2]),
                    shape: shape.to_string(),
                },
                graph,
                stack,
            );
            parsed = Some((id, &text[c[0].len()..]));
            break;
        }
    }
    let (id, mut rest) = if let Some(pair) = parsed {
        pair
    } else {
        let matched = ID
            .find(text)
            .ok_or_else(|| format!("Expected node near {text:?}"))?;
        let id = matched.as_str().to_owned();
        if !graph.nodes.iter().any(|n| n.id == id) {
            register(
                Node {
                    id: id.clone(),
                    label: id.clone(),
                    shape: "rectangle".into(),
                },
                graph,
                stack,
            );
        }
        (id, &text[matched.end()..])
    };
    if let Some(c) = CLASS.captures(rest) {
        graph.class_assignments.insert(id.clone(), c[1].to_owned());
        rest = &rest[c[0].len()..];
    }
    Ok((id, rest))
}
fn node_group<'a>(
    text: &'a str,
    graph: &mut Graph,
    stack: &mut [Subgraph],
) -> Result<(Vec<String>, &'a str), String> {
    let (id, mut rest) = node(text, graph, stack)?;
    let mut ids = vec![id];
    rest = rest.trim();
    while let Some(after) = rest.strip_prefix('&') {
        let (id, next) = node(after.trim(), graph, stack)?;
        ids.push(id);
        rest = next.trim();
    }
    Ok((ids, rest))
}
fn edge_line(text: &str, graph: &mut Graph, stack: &mut [Subgraph]) -> Result<(), String> {
    let (mut prev, mut rest) = node_group(text, graph, stack)?;
    while !rest.is_empty() {
        let (start, end, label, kind, len) = if let Some(c) = ARROW.captures(rest) {
            (
                c.get(1).is_some(),
                c[2].ends_with('>'),
                c.get(3).map(|m| normalize_br_tags(m.as_str().trim())),
                if c[2].contains('.') {
                    "dotted"
                } else if c[2].contains('=') {
                    "thick"
                } else {
                    "solid"
                },
                c[0].len(),
            )
        } else if let Some(c) = TEXT_ARROW.captures(rest) {
            (
                c.get(1).is_some(),
                c[4].ends_with('>'),
                Some(normalize_br_tags(c[3].trim())),
                if c[2].contains('.') || c[4].contains('.') {
                    "dotted"
                } else if c[2].contains('=') || c[4].contains('=') {
                    "thick"
                } else {
                    "solid"
                },
                c[0].len(),
            )
        } else {
            return Err(format!("Unsupported syntax near {rest:?}"));
        };
        let (next, remaining) = node_group(rest[len..].trim(), graph, stack)?;
        if prev
            .len()
            .checked_mul(next.len())
            .is_none_or(|count| count > 200 - graph.edges.len())
        {
            return Err("At most 200 edges are supported".into());
        }
        for source in &prev {
            for target in &next {
                graph.edges.push(Edge {
                    source: source.clone(),
                    target: target.clone(),
                    label: label.clone().filter(|s| !s.is_empty()),
                    style: kind.into(),
                    has_arrow_start: start,
                    has_arrow_end: end,
                });
            }
        }
        if graph.edges.len() > 200 {
            return Err("At most 200 edges are supported".into());
        }
        prev = next;
        rest = remaining;
    }
    Ok(())
}
/// Parse the supported flowchart grammar, retaining nested subgraph membership.
/// Other Mermaid diagram families are deliberately rejected.
pub fn parse(source: &str) -> Result<Graph, String> {
    if source.len() > 65_536 {
        return Err("Diagram exceeds the 64 KiB input limit".into());
    }
    let mut lines = source
        .lines()
        .enumerate()
        .filter(|(_, s)| !s.trim().is_empty() && !s.trim().starts_with("%%"));
    let (_, header) = lines.next().ok_or("Empty Mermaid diagram")?;
    let captures = HEADER
        .captures(header.trim())
        .ok_or("Expected flowchart or graph header with TD, TB, LR, BT, or RL")?;
    let mut graph = Graph {
        direction: captures[1].to_uppercase(),
        nodes: vec![],
        edges: vec![],
        subgraphs: vec![],
        class_defs: BTreeMap::new(),
        class_assignments: BTreeMap::new(),
        node_styles: BTreeMap::new(),
        link_styles: BTreeMap::new(),
    };
    let mut stack: Vec<Subgraph> = vec![];
    let mut group_ids = std::collections::BTreeSet::new();
    for (line_number, raw) in lines {
        let line = raw.trim().trim_end_matches(';').trim();
        let result = (|| -> Result<(), String> {
            if let Some(rest) = line.strip_prefix("subgraph ") {
                let (id, label) = if let Some(c) = SUBGRAPH.captures(rest.trim()) {
                    (c[1].to_string(), normalize_br_tags(&c[2]))
                } else {
                    (
                        rest.split_whitespace()
                            .collect::<Vec<_>>()
                            .join("_")
                            .chars()
                            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect(),
                        normalize_br_tags(rest.trim()),
                    )
                };
                if id.is_empty() || !group_ids.insert(id.clone()) {
                    return Err("Subgraphs require unique nonempty IDs".into());
                }
                if stack.len() >= 16 {
                    return Err("Subgraph nesting exceeds 16 levels".into());
                }
                stack.push(Subgraph {
                    id,
                    label,
                    node_ids: vec![],
                    children: vec![],
                    direction: None,
                });
                return Ok(());
            }
            if line == "end" {
                let group = stack.pop().ok_or("Unexpected subgraph end")?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(group);
                } else {
                    graph.subgraphs.push(group);
                }
                return Ok(());
            }
            if let Some(direction) = line.strip_prefix("direction ") {
                let direction = direction.to_uppercase();
                if !["TD", "TB", "LR", "BT", "RL"].contains(&direction.as_str()) {
                    return Err("Invalid subgraph direction".into());
                }
                stack
                    .last_mut()
                    .ok_or("Direction override requires a subgraph")?
                    .direction = Some(direction);
                return Ok(());
            }
            for keyword in ["classDef ", "class ", "style ", "linkStyle "] {
                if let Some(rest) = line.strip_prefix(keyword) {
                    let (ids, props) = rest
                        .split_once(char::is_whitespace)
                        .ok_or("Missing style or class assignment")?;
                    if keyword == "class " {
                        for id in ids.split(',') {
                            graph
                                .class_assignments
                                .insert(id.into(), props.trim().into());
                        }
                    } else {
                        let values = style(props)?;
                        let target = match keyword {
                            "classDef " => &mut graph.class_defs,
                            "style " => &mut graph.node_styles,
                            _ => &mut graph.link_styles,
                        };
                        for id in ids.split(',') {
                            target.entry(id.into()).or_default().extend(values.clone());
                        }
                    }
                    return Ok(());
                }
            }
            edge_line(line, &mut graph, &mut stack)
        })();
        result.map_err(|e| format!("Line {}: {e}", line_number + 1))?;
        if graph.nodes.len() > 100 {
            return Err("At most 100 nodes are supported".into());
        }
    }
    if !stack.is_empty() {
        return Err("Unclosed subgraph".into());
    }
    // Group endpoints are represented as bare nodes during parsing, as in upstream.
    // The layout adapter removes these pseudo-nodes when building compound nodes.
    if graph.nodes.is_empty() {
        return Err("Diagram has no nodes".into());
    }
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nesting_keeps_local_membership() {
        let g=parse("flowchart LR\nsubgraph Outer\n A[Alpha]\n subgraph Inner\n B[Beta]\n end\n A --> B\nend\n B --> C[Gamma]").unwrap();
        assert_eq!(g.subgraphs[0].node_ids, vec!["A"]);
        assert_eq!(g.subgraphs[0].children[0].node_ids, vec!["B"]);
        assert_eq!(g.edges.len(), 2);
    }
    #[test]
    fn rejects_partial_parse() {
        for s in [
            "flowchart LR\nA -->",
            "flowchart LR\nsubgraph X\nA",
            "flowchart LR\nend",
            "sequenceDiagram\nA->>B: hello",
        ] {
            assert!(parse(s).is_err(), "{s}");
        }
    }
    #[test]
    fn arrows_shapes_and_styles() {
        let g=parse("flowchart TD\nA{Ready} & B((Wait)) -->|Yes| C([Done])\nclassDef ok fill:#00ff00\nclass C ok").unwrap();
        assert_eq!(g.edges.len(), 2);
        assert_eq!(g.nodes[0].shape, "diamond");
        assert_eq!(g.class_assignments["C"], "ok");
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    #[test]
    fn limits_parallel_edges_before_expansion() {
        let group = vec!["A"; 1000].join(" & ");
        assert!(parse(&format!("flowchart LR\n{group} --> {group}")).is_err());
    }
    #[test]
    fn closing_operator_controls_text_arrow_style() {
        let g = parse("flowchart LR\nA -- label ==> B\nB -- label .-> C").unwrap();
        assert_eq!(g.edges[0].style, "thick");
        assert_eq!(g.edges[1].style, "dotted");
    }
}
