//! Shared native diagram rendering for the reader and published reports.
use beautiful_mermaid_rs::{RenderOptions, theme};

fn diagram_options(dark: bool) -> RenderOptions {
    let mut colors = theme::theme("github-light").expect("bundled diagram palette");
    if dark {
        // Match the app's neutral surfaces and warm accent, including glass mode.
        colors.bg = "#303234".into();
        colors.fg = "#f0f0f1".into();
        colors.muted = Some("#c2c3c5".into());
        colors.surface = Some("#414345".into());
        colors.border = Some("#777a7e".into());
        colors.line = Some("#a0a2a5".into());
        colors.accent = Some("#c8a875".into());
    } else {
        colors.muted = Some("#57606a".into());
        colors.border = Some("#b6c2cf".into());
        colors.line = Some("#73849a".into());
    }
    RenderOptions {
        colors,
        transparent: dark,
        ..RenderOptions::default()
    }
}

pub fn render_pair(source: &str) -> Result<(String, String), String> {
    if source.len() > 16_384 || source.lines().count() > 256 {
        return Err("Diagram exceeds rendering limits".into());
    }
    let options = diagram_options(false);
    let dark = diagram_options(true);
    // Prefer the new renderer. Preserve existing syntax/type support while the
    // independent library expands; a failed render never changes report source.
    if let (Ok(light), Ok(dark)) = (
        beautiful_mermaid_rs::render(source, &options),
        beautiful_mermaid_rs::render(source, &dark),
    ) {
        return Ok((light, dark));
    }
    let parsed = mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|e| e.to_string())?;
    let options = mermaid_rs_renderer::RenderOptions::default();
    let layout =
        mermaid_rs_renderer::compute_layout(&parsed.graph, &options.theme, &options.layout);
    let dark = mermaid_rs_renderer::Theme {
        font_family: options.theme.font_family.clone(),
        font_size: options.theme.font_size,
        ..mermaid_rs_renderer::Theme::dark()
    };
    Ok((
        mermaid_rs_renderer::render_svg(&layout, &options.theme, &options.layout),
        mermaid_rs_renderer::render_svg(&layout, &dark, &options.layout),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reader_uses_native_diagram_theme_and_keeps_legacy_types() {
        for source in [
            include_str!("../experiments/rust-mermaid/fixtures/gates.mmd"),
            include_str!("../experiments/rust-mermaid/fixtures/state.mmd"),
            include_str!("../experiments/rust-mermaid/fixtures/sequence.mmd"),
            include_str!("../experiments/rust-mermaid/fixtures/class.mmd"),
            include_str!("../experiments/rust-mermaid/fixtures/er.mmd"),
            include_str!("../experiments/rust-mermaid/fixtures/xychart.mmd"),
        ] {
            let (light, dark) = render_pair(source).unwrap();
            let expected = beautiful_mermaid_rs::render(source, &diagram_options(true)).unwrap();
            assert_eq!(dark, expected);
            assert!(light.contains("<svg"));
        }
        assert!(render_pair("pie\n\"Complete\" : 2\n\"Pending\" : 1").is_ok());
        assert!(render_pair("invalid diagram").is_err());
    }
}
