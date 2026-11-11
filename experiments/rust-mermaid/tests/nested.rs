use beautiful_mermaid_rs::{RenderOptions, layout, parser, types::*};
use std::collections::BTreeMap;
fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/fixtures/{name}.mmd",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn flatten<'a>(groups: &'a [PositionedGroup], out: &mut BTreeMap<&'a str, &'a PositionedGroup>) {
    for g in groups {
        out.insert(&g.id, g);
        flatten(&g.children, out);
    }
}
fn inside(p: Point, x: f64, y: f64, w: f64, h: f64) -> bool {
    let e = 0.01;
    p.x >= x - e && p.y >= y - e && p.x <= x + w + e && p.y <= y + h + e
}
fn validate_membership(
    groups: &[Subgraph],
    actual: &BTreeMap<&str, &PositionedGroup>,
    nodes: &BTreeMap<&str, &PositionedNode>,
) {
    for group in groups {
        let bounds = actual[group.id.as_str()];
        for id in &group.node_ids {
            let n = nodes[id.as_str()];
            assert!(
                inside(
                    Point { x: n.x, y: n.y },
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height
                ),
                "{id} starts outside {}",
                group.id
            );
            assert!(
                inside(
                    Point {
                        x: n.x + n.width,
                        y: n.y + n.height
                    },
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height
                ),
                "{id} ends outside {}",
                group.id
            );
        }
        for child in &group.children {
            let c = actual[child.id.as_str()];
            assert!(inside(
                Point { x: c.x, y: c.y },
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height
            ));
            assert!(inside(
                Point {
                    x: c.x + c.width,
                    y: c.y + c.height
                },
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height
            ));
        }
        validate_membership(&group.children, actual, nodes);
    }
}
#[test]
fn nested_groups_and_cross_boundary_edges_remain_connected() {
    for name in ["nested", "direction"] {
        let source = fixture(name);
        let parsed = parser::parse(&source).unwrap();
        let output = layout(&source, &RenderOptions::default()).unwrap();
        assert_eq!(output.edges.len(), parsed.edges.len());
        assert_eq!(output.nodes.len(), parsed.nodes.len());
        let nodes: BTreeMap<_, _> = output.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut groups = BTreeMap::new();
        flatten(&output.groups, &mut groups);
        validate_membership(&parsed.subgraphs, &groups, &nodes);
        for edge in &output.edges {
            assert!(edge.points.len() >= 2);
            let first = *edge.points.first().unwrap();
            let last = *edge.points.last().unwrap();
            let a = nodes[edge.source.as_str()];
            let b = nodes[edge.target.as_str()];
            assert!(
                inside(first, a.x, a.y, a.width, a.height),
                "{name}: {} starts detached: {first:?}, node {a:?}",
                edge.source
            );
            assert!(
                inside(last, b.x, b.y, b.width, b.height),
                "{name}: {} ends detached: {last:?}, node {b:?}",
                edge.target
            );
            for p in &edge.points {
                assert!(p.x.is_finite() && p.y.is_finite());
                assert!(
                    inside(*p, 0.0, 0.0, output.width, output.height),
                    "{name}: off-canvas {p:?}"
                );
            }
        }
    }
}

#[test]
fn nested_direction_override_is_explicitly_rejected() {
    let source = "flowchart LR\nsubgraph Outer\nsubgraph Inner\ndirection TB\nA --> B\nend\nend";
    let error = layout(source, &RenderOptions::default()).unwrap_err();
    assert!(error.contains("Direction overrides on nested subgraphs"));
}
