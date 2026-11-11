//! Native resolution of Beautiful Mermaid 1.1.3 theme colors.
//!
//! Ported from Beautiful Mermaid, Copyright (c) 2026 Craft Docs, MIT.
//! CSS variables/color-mix are resolved to RGB for standalone native SVG decoders.
use crate::types::DiagramColors;

pub(crate) struct Palette {
    pub bg: String,
    pub text: String,
    pub secondary: String,
    pub text_muted: String,
    pub text_faint: String,
    pub key_badge: String,
    pub line: String,
    pub arrow: String,
    pub node_fill: String,
    pub node_stroke: String,
    pub group_header: String,
    pub inner_stroke: String,
}

fn rgb(input: &str) -> Result<[u8; 3], String> {
    let named = match input.to_ascii_lowercase().as_str() {
        "white" => "#ffffff",
        "black" => "#000000",
        "red" => "#ff0000",
        "green" => "#008000",
        "blue" => "#0000ff",
        "yellow" => "#ffff00",
        "gray" | "grey" => "#808080",
        "orange" => "#ffa500",
        "purple" => "#800080",
        _ => input,
    };
    let Some(hex) = named.strip_prefix('#') else {
        return Err(format!("Unsupported color {input:?}: use #RGB or #RRGGBB."));
    };
    if !hex.is_ascii() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid hexadecimal color.".into());
    }
    match hex.len() {
        3 => Ok([0, 1, 2].map(|i| u8::from_str_radix(&hex[i..i + 1], 16).unwrap() * 17)),
        6 => Ok([0, 2, 4].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())),
        _ => Err("Expected #RGB or #RRGGBB color.".into()),
    }
}
fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}
pub(crate) fn color(input: &str) -> Result<String, String> {
    if input == "none" {
        return Ok(input.into());
    }
    Ok(hex(rgb(input)?))
}
fn mix(fg: [u8; 3], bg: [u8; 3], weight: f64) -> String {
    hex([0, 1, 2]
        .map(|i| (f64::from(fg[i]) * weight + f64::from(bg[i]) * (1.0 - weight)).round() as u8))
}
pub(crate) fn resolve(colors: &DiagramColors) -> Result<Palette, String> {
    let fg = rgb(&colors.fg)?;
    let bg = rgb(&colors.bg)?;
    let role = |override_color: &Option<String>, weight| match override_color {
        Some(value) => color(value),
        None => Ok(mix(fg, bg, weight)),
    };
    Ok(Palette {
        bg: hex(bg),
        text: hex(fg),
        secondary: role(&colors.muted, 0.60)?,
        text_muted: role(&colors.muted, 0.40)?,
        text_faint: mix(fg, bg, 0.25),
        key_badge: mix(fg, bg, 0.10),
        line: role(&colors.line, 0.50)?,
        arrow: role(&colors.accent, 0.85)?,
        node_fill: role(&colors.surface, 0.03)?,
        node_stroke: role(&colors.border, 0.20)?,
        group_header: mix(fg, bg, 0.05),
        inner_stroke: mix(fg, bg, 0.12),
    })
}

/// Look up one of the 15 palettes in Beautiful Mermaid 1.1.3.
pub fn theme(name: &str) -> Option<DiagramColors> {
    let (bg, fg, line, accent, muted) = match name {
        "zinc-light" => ("#FFFFFF", "#27272A", None, None, None),
        "zinc-dark" => ("#18181B", "#FAFAFA", None, None, None),
        "tokyo-night" => (
            "#1a1b26",
            "#a9b1d6",
            Some("#3d59a1"),
            Some("#7aa2f7"),
            Some("#565f89"),
        ),
        "tokyo-night-storm" => (
            "#24283b",
            "#a9b1d6",
            Some("#3d59a1"),
            Some("#7aa2f7"),
            Some("#565f89"),
        ),
        "tokyo-night-light" => (
            "#d5d6db",
            "#343b58",
            Some("#34548a"),
            Some("#34548a"),
            Some("#9699a3"),
        ),
        "catppuccin-mocha" => (
            "#1e1e2e",
            "#cdd6f4",
            Some("#585b70"),
            Some("#cba6f7"),
            Some("#6c7086"),
        ),
        "catppuccin-latte" => (
            "#eff1f5",
            "#4c4f69",
            Some("#9ca0b0"),
            Some("#8839ef"),
            Some("#9ca0b0"),
        ),
        "nord" => (
            "#2e3440",
            "#d8dee9",
            Some("#4c566a"),
            Some("#88c0d0"),
            Some("#616e88"),
        ),
        "nord-light" => (
            "#eceff4",
            "#2e3440",
            Some("#aab1c0"),
            Some("#5e81ac"),
            Some("#7b88a1"),
        ),
        "dracula" => (
            "#282a36",
            "#f8f8f2",
            Some("#6272a4"),
            Some("#bd93f9"),
            Some("#6272a4"),
        ),
        "github-light" => (
            "#ffffff",
            "#1f2328",
            Some("#d1d9e0"),
            Some("#0969da"),
            Some("#59636e"),
        ),
        "github-dark" => (
            "#0d1117",
            "#e6edf3",
            Some("#3d444d"),
            Some("#4493f8"),
            Some("#9198a1"),
        ),
        "solarized-light" => (
            "#fdf6e3",
            "#657b83",
            Some("#93a1a1"),
            Some("#268bd2"),
            Some("#93a1a1"),
        ),
        "solarized-dark" => (
            "#002b36",
            "#839496",
            Some("#586e75"),
            Some("#268bd2"),
            Some("#586e75"),
        ),
        "one-dark" => (
            "#282c34",
            "#abb2bf",
            Some("#4b5263"),
            Some("#c678dd"),
            Some("#5c6370"),
        ),
        _ => return None,
    };
    Some(DiagramColors {
        bg: bg.into(),
        fg: fg.into(),
        line: line.map(str::to_owned),
        accent: accent.map(str::to_owned),
        muted: muted.map(str::to_owned),
        surface: None,
        border: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_upstream_theme_mix_ratios() {
        let p = resolve(&DiagramColors::default()).unwrap();
        assert_eq!(p.node_fill, "#f9f9f9");
        assert_eq!(p.node_stroke, "#d4d4d4");
        assert_eq!(p.secondary, "#7d7d7f");
        assert_eq!(p.group_header, "#f4f4f4");
    }
    #[test]
    fn rejects_external_and_executable_color_values() {
        assert!(color("url(https://example.com/image.svg)").is_err());
        assert!(color("var(--color)").is_err());
        assert!(color("#💥").is_err());
        assert_eq!(color("#f00").unwrap(), "#ff0000");
    }
}
