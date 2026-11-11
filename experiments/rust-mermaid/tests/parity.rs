use beautiful_mermaid_rs::{RenderOptions, layout, parser};
use serde_json::Value;
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
fn source(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/fixtures/{name}.mmd",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn compare(actual: &Value, expected: &Value, path: &str, tolerance: f64) {
    match expected {
        Value::Number(n) => assert!(
            (actual.as_f64().unwrap_or(f64::INFINITY) - n.as_f64().unwrap()).abs() < tolerance,
            "{path}: {actual} != {expected}"
        ),
        Value::Array(values) => {
            assert_eq!(
                actual.as_array().map(Vec::len),
                Some(values.len()),
                "{path}"
            );
            for (i, v) in values.iter().enumerate() {
                compare(&actual[i], v, &format!("{path}[{i}]"), tolerance);
            }
        }
        Value::Object(values) => {
            for (k, v) in values {
                compare(&actual[k], v, &format!("{path}.{k}"), tolerance);
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn flowchart_inputs_match_pinned_upstream() {
    for name in ["gates", "cycle", "nested", "direction", "styles"] {
        let graph = parser::parse(&source(name)).unwrap();
        let input = layout::build_elk_graph(&graph, &RenderOptions::default()).unwrap();
        compare(&input, &reference(name)["input"], name, 1e-9);
    }
}
#[test]
fn positioned_graphs_match_pinned_upstream() {
    for name in ["gates", "cycle", "direction", "styles"] {
        let result = layout(&source(name), &RenderOptions::default()).unwrap();
        compare(
            &serde_json::to_value(result).unwrap(),
            &reference(name)["positioned"],
            name,
            1e-3,
        );
    }
}
