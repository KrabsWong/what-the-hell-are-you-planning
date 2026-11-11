use beautiful_mermaid_rs::{RenderOptions, class, er, theme};
fn source(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/fixtures/{name}.mmd",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
#[test]
fn class_members_relationships_and_marker_sides_survive() {
    let source = source("class");
    let d = class::parse(&source).unwrap();
    assert_eq!(d.classes.len(), 3);
    assert_eq!(d.relationships.len(), 3);
    assert_eq!(d.classes[0].annotation.as_deref(), Some("abstract"));
    assert_eq!(d.classes[0].methods[0].params.as_deref(), Some("food"));
    assert_eq!(d.relationships[0].kind, "inheritance");
    assert_eq!(d.relationships[0].marker_at, "from");
    assert_eq!(d.relationships[1].to_cardinality.as_deref(), Some("*"));
    for palette in ["zinc-light", "zinc-dark"] {
        let svg = class::render(
            &source,
            &RenderOptions {
                colors: theme::theme(palette).unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(svg.contains("marker-start=\"url(#cls-inherit)\""));
        assert!(svg.contains("class-node"));
        assert!(svg.contains("eat(food)"));
        assert!(svg.contains("stroke-dasharray=\"6 4\""));
    }
}
#[test]
fn class_supports_each_marker_orientation() {
    for (arrow, kind, side) in [
        ("<|--", "inheritance", "from"),
        ("--|>", "inheritance", "to"),
        ("*--", "composition", "from"),
        ("--*", "composition", "to"),
        ("o--", "aggregation", "from"),
        ("--o", "aggregation", "to"),
        ("<|..", "realization", "from"),
        ("..|>", "realization", "to"),
        ("<..", "dependency", "from"),
        ("..>", "dependency", "to"),
        ("<--", "association", "from"),
        ("-->", "association", "to"),
    ] {
        let d = class::parse(&format!("classDiagram\nA {arrow} B")).unwrap();
        assert_eq!(d.relationships[0].kind, kind);
        assert_eq!(d.relationships[0].marker_at, side);
    }
}
#[test]
fn er_keys_comments_and_cardinality_survive() {
    let source = source("er");
    let d = er::parse(&source).unwrap();
    assert_eq!(d.entities.len(), 3);
    assert_eq!(d.relationships.len(), 2);
    assert_eq!(d.entities[0].attributes[0].keys, vec!["PK"]);
    assert_eq!(
        d.entities[0].attributes[2].comment.as_deref(),
        Some("Contact address")
    );
    assert_eq!(d.relationships[0].cardinality1, "one");
    assert_eq!(d.relationships[0].cardinality2, "zero-many");
    assert!(!d.relationships[1].identifying);
    let svg = er::render(&source, &RenderOptions::default()).unwrap();
    assert!(svg.contains("<title>Contact address</title>"));
    assert!(svg.contains("(no attributes)"));
    assert!(svg.contains("<circle"));
    assert!(svg.contains("stroke-dasharray=\"6 4\""));
}
#[test]
fn malformed_unsupported_or_unclosed_diagrams_fail() {
    for input in [
        "classDiagram\nnamespace N {\nclass A\n}",
        "classDiagram\nclass A {\n+int id",
        "classDiagram\nA ??? B",
    ] {
        assert!(class::parse(input).is_err());
    }
    for input in [
        "erDiagram\nA {\nint id",
        "erDiagram\nA ?? B",
        "erDiagram\nA {\nint id INVALID\n}",
    ] {
        assert!(er::parse(input).is_err());
    }
}
#[test]
fn user_labels_remain_xml_escaped() {
    let svg = class::render(
        "classDiagram\nclass A {\n+String <script>\n}",
        &RenderOptions::default(),
    )
    .unwrap();
    assert!(!svg.contains("<script>"));
    assert!(svg.contains("&lt;script&gt;"));
}
fn compare(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    use serde_json::Value;
    match expected {
        Value::Number(n) => assert!(
            (actual.as_f64().unwrap_or(f64::INFINITY) - n.as_f64().unwrap()).abs() < 1e-3,
            "{path}: {actual} != {expected}"
        ),
        Value::Array(a) => {
            assert_eq!(actual.as_array().map(Vec::len), Some(a.len()), "{path}");
            for (i, v) in a.iter().enumerate() {
                compare(&actual[i], v, &format!("{path}[{i}]"));
            }
        }
        Value::Object(o) => {
            for (k, v) in o {
                compare(&actual[k], v, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn positioned_class_and_er_match_pinned_upstream() {
    for name in ["class", "er"] {
        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(format!(
                "{}/fixtures/reference/{name}.json",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .unwrap();
        let actual = if name == "class" {
            serde_json::to_value(class::layout(&source(name), &RenderOptions::default()).unwrap())
                .unwrap()
        } else {
            serde_json::to_value(er::layout(&source(name), &RenderOptions::default()).unwrap())
                .unwrap()
        };
        compare(&actual, &expected["positioned"], name);
    }
}

#[test]
fn user_root_identifiers_do_not_collide_with_layout_container() {
    let class_source = "classDiagram\nroot --> root_\nroot_ --> root__";
    let er_source = "erDiagram\nroot ||--o{ root_ : contains\nroot_ ||--o{ root__ : contains";
    let options = RenderOptions::default();
    let classes = class::layout(class_source, &options).unwrap();
    let entities = er::layout(er_source, &options).unwrap();
    assert_eq!(classes.classes.len(), 3);
    assert_eq!(classes.relationships.len(), 2);
    assert_eq!(entities.entities.len(), 3);
    assert_eq!(entities.relationships.len(), 2);
    for id in ["root", "root_", "root__"] {
        assert!(classes.classes.iter().any(|node| node.class.id == id));
        assert!(entities.entities.iter().any(|node| node.entity.id == id));
    }
    assert!(
        class::render(class_source, &options)
            .unwrap()
            .contains("data-id=\"root\"")
    );
    assert!(
        er::render(er_source, &options)
            .unwrap()
            .contains("data-id=\"root\"")
    );
}
