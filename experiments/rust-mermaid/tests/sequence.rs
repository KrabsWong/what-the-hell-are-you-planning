use beautiful_mermaid_rs::{RenderOptions, sequence};
use serde_json::Value;
fn compare(actual: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Number(n) => assert!(
            (actual.as_f64().unwrap() - n.as_f64().unwrap()).abs() < 1e-8,
            "{path}: {actual} != {expected}"
        ),
        Value::Array(values) => {
            assert_eq!(actual.as_array().unwrap().len(), values.len(), "{path}");
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
fn sequence_participant_geometry_matches_upstream() {
    let expected: Value =
        serde_json::from_str(include_str!("../fixtures/reference/sequence-basic.json")).unwrap();
    let actual = sequence::layout(
        include_str!("../fixtures/sequence-basic.mmd"),
        &RenderOptions::default(),
    )
    .unwrap();
    compare(
        &serde_json::to_value(actual).unwrap(),
        &expected["positioned"],
        "sequence",
    );
}
#[test]
fn initial_and_multiline_notes_are_preserved() {
    let output = sequence::layout(
        "sequenceDiagram\nNote over A: Before<br/>the request\nA->>B: Request",
        &RenderOptions::default(),
    )
    .unwrap();
    assert_eq!(output.notes.len(), 1);
    assert!(output.notes[0].height > 23.0);
    assert!(output.notes[0].y + output.notes[0].height < output.messages[0].y);
}
#[test]
fn unsupported_commands_are_not_silently_lost() {
    for command in [
        "autonumber",
        "activate A",
        "deactivate A",
        "end",
        "A-xB: Destroy",
        "loop unclosed\nA->>B: Message",
    ] {
        assert!(
            sequence::render(
                &format!("sequenceDiagram\nparticipant A\n{command}"),
                &RenderOptions::default()
            )
            .is_err(),
            "{command}"
        );
    }
}
#[test]
fn nested_activations_and_self_messages_keep_geometry() {
    let output = sequence::layout(
        "sequenceDiagram\nA->>+B: Start\nB->>+B: Nested\nB-->>-B: Return\nB-->>-A: Complete",
        &RenderOptions::default(),
    )
    .unwrap();
    assert_eq!(output.activations.len(), 2);
    assert_eq!(output.activations[0].x - output.activations[1].x, 4.0);
    assert!(output.messages[1].is_self);
    assert!(output.messages[2].y - output.messages[1].y >= 70.0);
}
#[test]
fn sequence_svg_escapes_untrusted_text() {
    let svg = sequence::render(
        "sequenceDiagram\nparticipant A as <script>alert(1)</script>\nA->>B: <img src=x>",
        &RenderOptions::default(),
    )
    .unwrap();
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("<img"));
    assert!(svg.contains("&lt;script&gt;"));
}

#[test]
fn actor_labels_and_overlapping_dividers_have_clearance() {
    let output = sequence::layout(
        include_str!("../fixtures/sequence.mmd"),
        &RenderOptions::default(),
    )
    .unwrap();
    let actor = &output.actors[0];
    let actor_label_y = actor.y + actor.height + 14.0;
    let first_message_label_y = output.messages[0].y - 10.0;
    assert!(first_message_label_y - actor_label_y >= 20.0);
    let block = output.blocks.iter().find(|b| b.kind == "alt").unwrap();
    let divider = &block.dividers[0];
    let message = output.messages.last().unwrap();
    assert!((message.y - 10.0) - (divider.y + 14.0) >= 20.0);
    let reference: Value =
        serde_json::from_str(include_str!("../fixtures/reference/sequence.json")).unwrap();
    assert_eq!(
        output.messages[0].y
            - reference["positioned"]["messages"][0]["y"]
                .as_f64()
                .unwrap(),
        24.0
    );
}

#[test]
fn block_tabs_paint_above_activation_bars() {
    let svg = beautiful_mermaid_rs::render(
        "sequenceDiagram\nA->>+B: Begin\nloop Work\nB->>A: Update\nend\nB-->>-A: Done",
        &beautiful_mermaid_rs::RenderOptions::default(),
    )
    .unwrap();
    assert!(svg.find("class=\"activation\"").unwrap() < svg.find("class=\"block\"").unwrap());
}

#[test]
fn nested_control_blocks_fail_instead_of_hiding_inner_labels() {
    assert!(
        beautiful_mermaid_rs::render(
            "sequenceDiagram\nloop Outer\nloop Inner\nA->>B: Message\nend\nend",
            &beautiful_mermaid_rs::RenderOptions::default(),
        )
        .unwrap_err()
        .contains("Nested sequence control blocks")
    );
}
