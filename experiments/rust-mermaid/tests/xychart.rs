use beautiful_mermaid_rs::{RenderOptions, xychart};
use serde_json::Value;
fn compare(actual: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Number(n) => assert!(
            (actual.as_f64().unwrap_or(f64::INFINITY) - n.as_f64().unwrap()).abs() < 1e-8,
            "{path}: {actual} != {expected}"
        ),
        Value::Array(values) => {
            assert_eq!(
                actual.as_array().map(Vec::len),
                Some(values.len()),
                "{path}"
            );
            for (i, v) in values.iter().enumerate() {
                compare(&actual[i], v, &format!("{path}[{i}]"));
            }
        }
        Value::Object(values) => {
            for (k, v) in values {
                compare(&actual[k], v, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn mixed_xy_layout_matches_upstream() {
    let source = include_str!("../fixtures/xychart.mmd");
    let parsed = xychart::parse(source).unwrap();
    let positioned = xychart::layout(&parsed).unwrap();
    let reference: Value =
        serde_json::from_str(include_str!("../fixtures/reference/xychart.json")).unwrap();
    compare(
        &serde_json::to_value(parsed).unwrap(),
        &reference["graph"],
        "parsed",
    );
    compare(
        &serde_json::to_value(positioned).unwrap(),
        &reference["positioned"],
        "positioned",
    );
}
#[test]
fn horizontal_nonmonotonic_lines_remain_finite() {
    let svg = xychart::render(
        "xychart-beta horizontal\nx-axis [A,B,C,D]\ny-axis 0 --> 100\nline [50,50,10,90]",
        &RenderOptions::default(),
    )
    .unwrap();
    assert!(!svg.contains("NaN"));
    assert!(!svg.contains("inf"));
    assert!(svg.contains("xychart-line"));
}
#[test]
fn rejects_invalid_numeric_data_and_shape_mismatch() {
    for body in [
        "bar [NaN]",
        "bar [Infinity]",
        "bar [1junk]",
        "bar []",
        "bar [1,2]\nline [3]",
        "x-axis [A,B]\nbar [1]",
        "y-axis 5 --> 5\nbar [5]",
        "y-axis 0 --> 10\nbar [11]",
        "y-axis [A,B]\nbar [1,2]",
    ] {
        assert!(
            xychart::parse(&format!("xychart-beta\n{body}")).is_err(),
            "{body}"
        );
    }
}
#[test]
fn static_svg_escapes_labels_and_has_no_active_content() {
    let svg = xychart::render(
        "xychart-beta\ntitle \"<script>bad</script>\"\nx-axis [<g>,B]\nbar [1,2]",
        &RenderOptions::default(),
    )
    .unwrap();
    assert!(svg.contains("&lt;script&gt;"));
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("<style>"));
}
#[test]
fn auto_range_supports_constant_and_negative_data() {
    for values in ["5,5,5", "-10,-5,-7", "0,0,0"] {
        let chart = xychart::parse(&format!("xychart-beta\nbar [{values}]")).unwrap();
        let positioned = xychart::layout(&chart).unwrap();
        assert!(positioned.bars.iter().all(|b| b.height.is_finite()
            && b.height >= 0.0
            && b.y >= positioned.plot_area.y - 1e-6
            && b.y + b.height <= positioned.plot_area.y + positioned.plot_area.height + 1e-6));
    }
}

#[test]
fn horizontal_xy_layout_matches_upstream() {
    let parsed = xychart::parse(include_str!("../fixtures/xychart-horizontal.mmd")).unwrap();
    let positioned = xychart::layout(&parsed).unwrap();
    let reference: Value = serde_json::from_str(include_str!(
        "../fixtures/reference/xychart-horizontal.json"
    ))
    .unwrap();
    compare(
        &serde_json::to_value(parsed).unwrap(),
        &reference["graph"],
        "parsed",
    );
    compare(
        &serde_json::to_value(positioned).unwrap(),
        &reference["positioned"],
        "positioned",
    );
}
