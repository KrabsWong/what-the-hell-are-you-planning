//! State diagram parser ported from Beautiful Mermaid 1.1.3 (Craft Docs, MIT).
use crate::{metrics::normalize_br_tags, types::*};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};

fn register(graph: &mut Graph, stack: &mut [Subgraph], node: Node) {
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
fn declare(graph: &mut Graph, stack: &mut [Subgraph], node: Node) {
    if let Some(existing) = graph.nodes.iter_mut().find(|n| n.id == node.id) {
        existing.label = node.label.clone();
        existing.shape = node.shape.clone();
    }
    register(graph, stack, node);
}
/// Parse simple and composite states. Unsupported directives return errors.
pub fn parse(source: &str) -> Result<Graph, String> {
    if source.len() > 65_536 {
        return Err("Diagram exceeds the 64 KiB input limit".into());
    }
    let mut lines = source
        .lines()
        .enumerate()
        .filter(|(_, s)| !s.trim().is_empty() && !s.trim().starts_with("%%"));
    let (_, header) = lines.next().ok_or("Empty state diagram")?;
    if !["statediagram", "statediagram-v2"].contains(&header.trim().to_lowercase().as_str()) {
        return Err("Expected stateDiagram or stateDiagram-v2".into());
    }
    let mut graph = Graph {
        direction: "TD".into(),
        nodes: vec![],
        edges: vec![],
        subgraphs: vec![],
        class_defs: BTreeMap::new(),
        class_assignments: BTreeMap::new(),
        node_styles: BTreeMap::new(),
        link_styles: BTreeMap::new(),
    };
    let composite = Regex::new(r#"^state\s+(?:"([^"]+)"\s+as\s+)?([\w\p{L}]+)\s*\{$"#).unwrap();
    let alias = Regex::new(r#"^state\s+"([^"]+)"\s+as\s+([\w\p{L}]+)\s*$"#).unwrap();
    let transition =
        Regex::new(r"^(\[\*\]|[\w\p{L}-]+)\s*-->\s*(\[\*\]|[\w\p{L}-]+)(?:\s*:\s*(.+))?$").unwrap();
    let description = Regex::new(r"^([\w\p{L}-]+)\s*:\s*(.+)$").unwrap();
    // Reserve authored identifiers before generating pseudo-state IDs, including later declarations.
    let identifier = Regex::new(r"[\w\p{L}-]+").unwrap();
    let reserved: BTreeSet<&str> = identifier.find_iter(source).map(|m| m.as_str()).collect();
    let mut stack: Vec<Subgraph> = vec![];
    let mut ids = BTreeSet::new();
    let (mut start_count, mut end_count) = (0, 0);
    for (number, raw) in lines {
        let line = raw.trim().trim_end_matches(';').trim();
        let result = (|| -> Result<(), String> {
            if let Some(dir) = line.strip_prefix("direction ") {
                let dir = dir.to_uppercase();
                if !["TD", "TB", "LR", "BT", "RL"].contains(&dir.as_str()) {
                    return Err("Invalid state direction".into());
                }
                if let Some(group) = stack.last_mut() {
                    group.direction = Some(dir);
                } else {
                    graph.direction = dir;
                }
                return Ok(());
            }
            if let Some(rest) = line.strip_prefix("linkStyle ") {
                let (targets, properties) = rest
                    .split_once(char::is_whitespace)
                    .ok_or("Missing link style")?;
                let mut style = Style::new();
                for pair in properties.split(',') {
                    let (k, v) = pair
                        .split_once(':')
                        .ok_or("Expected style property:value")?;
                    style.insert(k.trim().into(), v.trim().into());
                }
                for target in targets.split(',') {
                    if target != "default" && target.parse::<usize>().is_err() {
                        return Err("Invalid link index".into());
                    }
                    graph
                        .link_styles
                        .entry(target.into())
                        .or_default()
                        .extend(style.clone());
                }
                return Ok(());
            }
            if let Some(c) = composite.captures(line) {
                let id = c[2].to_string();
                if !ids.insert(id.clone()) {
                    return Err("Duplicate composite state ID".into());
                }
                if stack.len() >= 16 {
                    return Err("Composite nesting exceeds 16 levels".into());
                }
                graph.nodes.retain(|n| n.id != id);
                // A transition before the declaration may have added the node to a parent.
                for parent in &mut stack {
                    parent.node_ids.retain(|n| n != &id);
                }
                stack.push(Subgraph {
                    id,
                    label: c
                        .get(1)
                        .map_or_else(|| c[2].into(), |m| normalize_br_tags(m.as_str())),
                    node_ids: vec![],
                    children: vec![],
                    direction: None,
                });
                return Ok(());
            }
            if line == "}" {
                let group = stack.pop().ok_or("Unexpected composite state end")?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(group);
                } else {
                    graph.subgraphs.push(group);
                }
                return Ok(());
            }
            if let Some(c) = alias.captures(line) {
                declare(
                    &mut graph,
                    &mut stack,
                    Node {
                        id: c[2].into(),
                        label: normalize_br_tags(&c[1]),
                        shape: "rounded".into(),
                    },
                );
                return Ok(());
            }
            if let Some(c) = transition.captures(line) {
                let mut endpoints = [c[1].to_string(), c[2].to_string()];
                for (index, id) in endpoints.iter_mut().enumerate() {
                    if id == "[*]" {
                        let (count, prefix, shape) = if index == 0 {
                            (&mut start_count, "_start", "state-start")
                        } else {
                            (&mut end_count, "_end", "state-end")
                        };
                        loop {
                            *count += 1;
                            *id = if *count == 1 {
                                prefix.into()
                            } else {
                                format!("{prefix}{count}")
                            };
                            if !reserved.contains(id.as_str()) {
                                break;
                            }
                        }
                        register(
                            &mut graph,
                            &mut stack,
                            Node {
                                id: id.clone(),
                                label: String::new(),
                                shape: shape.into(),
                            },
                        );
                    } else if !ids.contains(id) {
                        register(
                            &mut graph,
                            &mut stack,
                            Node {
                                id: id.clone(),
                                label: id.clone(),
                                shape: "rounded".into(),
                            },
                        );
                    }
                }
                graph.edges.push(Edge {
                    source: endpoints[0].clone(),
                    target: endpoints[1].clone(),
                    label: c.get(3).map(|m| normalize_br_tags(m.as_str().trim())),
                    style: "solid".into(),
                    has_arrow_start: false,
                    has_arrow_end: true,
                });
                return Ok(());
            }
            if let Some(c) = description.captures(line) {
                declare(
                    &mut graph,
                    &mut stack,
                    Node {
                        id: c[1].into(),
                        label: normalize_br_tags(c[2].trim()),
                        shape: "rounded".into(),
                    },
                );
                return Ok(());
            }
            Err(format!("Unsupported state syntax: {line}"))
        })();
        result.map_err(|e| format!("Line {}: {e}", number + 1))?;
        if graph.nodes.len() > 100 || graph.edges.len() > 200 {
            return Err("State diagram exceeds node/edge limits".into());
        }
    }
    if !stack.is_empty() {
        return Err("Unclosed composite state".into());
    }
    if graph.nodes.is_empty() {
        return Err("State diagram has no nodes".into());
    }
    Ok(graph)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_pseudostates_aliases_and_composites() {
        let g=parse("stateDiagram-v2\nstate \"Ready to run\" as Ready\n[*] --> Ready\nstate Work {\n[*] --> Active\nActive --> [*]\n}\nReady --> Work : begin\nWork --> [*]").unwrap();
        assert_eq!(g.subgraphs.len(), 1);
        assert_eq!(g.edges.len(), 5);
        assert_eq!(g.nodes[0].label, "Ready to run");
        assert!(g.nodes.iter().any(|n| n.id == "_start2"));
    }
    #[test]
    fn rejects_unsupported_and_unclosed_states() {
        assert!(parse("stateDiagram-v2\nnote right of A : Note").is_err());
        assert!(parse("stateDiagram-v2\nstate A {\n[*] --> B").is_err());
    }
}

#[cfg(test)]
mod declaration_tests {
    use super::*;
    #[test]
    fn declarations_after_transitions_update_labels() {
        for declaration in ["Ready: Ready to run", "state \"Ready to run\" as Ready"] {
            let graph = parse(&format!(
                "stateDiagram-v2\n[*] --> Ready\n{declaration}\nReady --> [*]"
            ))
            .unwrap();
            assert_eq!(
                graph.nodes.iter().find(|n| n.id == "Ready").unwrap().label,
                "Ready to run"
            );
        }
    }
    #[test]
    fn pseudostates_do_not_collide_with_authored_ids() {
        for source in [
            "_start: User start\n_end: User end\n[*] --> _start\n_end --> [*]",
            "[*] --> _start\n_end --> [*]\n_start: User start\n_end: User end",
        ] {
            let graph = parse(&format!("stateDiagram-v2\n{source}")).unwrap();
            assert_eq!(graph.nodes.len(), 4);
            assert!(graph.edges.iter().all(|e| e.source != e.target));
            assert_eq!(
                graph.nodes.iter().find(|n| n.id == "_start").unwrap().label,
                "User start"
            );
            assert_eq!(
                graph.nodes.iter().find(|n| n.id == "_end").unwrap().label,
                "User end"
            );
        }
    }
}
