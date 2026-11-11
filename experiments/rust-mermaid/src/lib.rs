//! Native Rust diagram rendering, ported from Beautiful Mermaid 1.1.3.
//!
//! ```
//! use beautiful_mermaid_rs::{render, RenderOptions};
//! let svg = render("flowchart LR\nA[Start] --> B[Done]", &RenderOptions::default())?;
//! assert!(svg.contains("<svg"));
//! # Ok::<(), String>(())
//! ```
//!
//! This preview supports flowchart, state, sequence, class, ER, and XY diagrams.
//! It does not claim complete Mermaid syntax compatibility or pixel-identical font
//! rendering. No JavaScript engine or network access is used at runtime.
pub mod class;
pub mod er;
pub mod layout;
pub mod metrics;
pub mod parser;
pub mod render;
pub mod sequence;
pub mod state;
pub mod theme;
pub mod types;
pub mod xychart;
pub use types::{DiagramColors, PositionedGraph, RenderOptions};

fn validate_options(options: &RenderOptions) -> Result<(), String> {
    for value in [
        options.padding,
        options.node_spacing,
        options.layer_spacing,
        options.component_spacing.unwrap_or(options.node_spacing),
    ] {
        if !value.is_finite() || !(0.0..=10_000.0).contains(&value) {
            return Err("Spacing must be finite and between 0 and 10000 pixels".into());
        }
    }
    Ok(())
}
/// Parse and lay out a flowchart or state diagram for native geometry consumers.
pub fn layout(source: &str, options: &RenderOptions) -> Result<PositionedGraph, String> {
    validate_options(options)?;
    let source = decode_xml(source);
    let header = source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .unwrap_or("");
    let graph = if header.eq_ignore_ascii_case("stateDiagram")
        || header.eq_ignore_ascii_case("stateDiagram-v2")
    {
        state::parse(&source)?
    } else {
        parser::parse(&source)?
    };
    layout::layout_graph(&graph, options)
}
/// Render a supported diagram into self-contained SVG without a JavaScript runtime.
pub fn render(source: &str, options: &RenderOptions) -> Result<String, String> {
    validate_options(options)?;
    if source.len() > 65_536 {
        return Err("Diagram exceeds the 64 KiB input limit".into());
    }
    let decoded = decode_xml(source);
    let header = decoded
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .unwrap_or("");
    if header.eq_ignore_ascii_case("sequenceDiagram") {
        return sequence::render(&decoded, options);
    }
    if header.eq_ignore_ascii_case("classDiagram") {
        return class::render(&decoded, options);
    }
    if header.eq_ignore_ascii_case("erDiagram") {
        return er::render(&decoded, options);
    }
    if header.to_ascii_lowercase().starts_with("xychart") {
        return xychart::render(&decoded, options);
    }
    render::render_svg(&layout(source, options)?, options)
}

// Match the upstream public API: decode once before parsing, then escape SVG output.
fn decode_xml(source: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    static ENTITY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"&(?:amp|lt|gt|quot|apos|#[0-9]+|#x[0-9a-fA-F]+);").unwrap());
    ENTITY
        .replace_all(source, |captures: &regex::Captures<'_>| {
            match &captures[0] {
                "&amp;" => "&".into(),
                "&lt;" => "<".into(),
                "&gt;" => ">".into(),
                "&quot;" => "\"".into(),
                "&apos;" => "'".into(),
                entity => {
                    let numeric = &entity[2..entity.len() - 1];
                    let code = if let Some(hex) = numeric.strip_prefix('x') {
                        u32::from_str_radix(hex, 16)
                    } else {
                        numeric.parse::<u32>()
                    };
                    code.ok()
                        .and_then(char::from_u32)
                        .filter(|c| *c != '\0')
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "\u{fffd}".into())
                }
            }
        })
        .into_owned()
}
#[cfg(test)]
mod api_tests {
    use super::*;
    #[test]
    fn decodes_xml_once_and_escapes_output() {
        assert_eq!(
            decode_xml("&amp;lt; &#65; &#x42; &unknown;"),
            "&lt; A B &unknown;"
        );
        let svg = render("flowchart LR\nA[\"A &amp; B\"]", &RenderOptions::default()).unwrap();
        assert!(svg.contains("A &amp; B"));
        assert!(!svg.contains("&amp;amp;"));
    }
    #[test]
    fn rejects_nonfinite_spacing() {
        let options = RenderOptions {
            padding: f64::NAN,
            ..RenderOptions::default()
        };
        assert!(render("flowchart LR\nA", &options).is_err());
    }
}
