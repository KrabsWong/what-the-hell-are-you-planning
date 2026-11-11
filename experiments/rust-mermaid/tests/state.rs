use beautiful_mermaid_rs::{RenderOptions, layout, state};
use serde_json::Value;
fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/fixtures/{name}.mmd",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn reference(name: &str) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(format!(
            "{}/fixtures/reference/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}
fn compare(a: &Value, b: &Value, path: &str) {
    match b {
        Value::Number(n) => assert!(
            (a.as_f64().unwrap_or(f64::INFINITY) - n.as_f64().unwrap()).abs() < 1e-3,
            "{path}: {a} != {b}"
        ),
        Value::Array(v) => {
            assert_eq!(a.as_array().map(Vec::len), Some(v.len()), "{path}");
            for (i, x) in v.iter().enumerate() {
                compare(&a[i], x, &format!("{path}[{i}]"));
            }
        }
        Value::Object(v) => {
            for (k, x) in v {
                compare(&a[k], x, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn simple_states_match_upstream_geometry() {
    let source = fixture("state");
    let options = RenderOptions::default();
    let parsed = state::parse(&source).unwrap();
    compare(
        &layout::build_elk_graph(&parsed, &options).unwrap(),
        &reference("state")["input"],
        "input",
    );
    compare(
        &serde_json::to_value(layout(&source, &options).unwrap()).unwrap(),
        &reference("state")["positioned"],
        "positioned",
    );
}
#[test]
fn composite_states_keep_all_nodes_and_transitions() {
    let source = fixture("state-nested");
    let graph = state::parse(&source).unwrap();
    let positioned = layout(&source, &RenderOptions::default()).unwrap();
    assert_eq!(positioned.nodes.len(), graph.nodes.len());
    assert_eq!(positioned.edges.len(), graph.edges.len());
    assert_eq!(positioned.groups.len(), 1);
    assert_eq!(positioned.groups[0].id, "Processing");
    for edge in positioned.edges {
        assert!(edge.points.len() >= 2);
        assert!(
            edge.points
                .iter()
                .all(|p| p.x.is_finite() && p.y.is_finite())
        );
    }
}
