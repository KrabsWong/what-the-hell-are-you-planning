//! Text measurement and label normalization ported from Beautiful Mermaid 1.1.3.
//!
//! Copyright (c) 2026 Craft Docs. Distributed under the MIT license; see
//! THIRD_PARTY_NOTICES.md. These are upstream's Inter-calibrated estimates,
//! not font shaping or platform font measurements.
use regex::Regex;
use std::sync::LazyLock;

pub const LINE_HEIGHT_RATIO: f64 = 1.3;
pub const NODE_LABEL_SIZE: f64 = 13.0;
pub const EDGE_LABEL_SIZE: f64 = 11.0;
pub const GROUP_HEADER_SIZE: f64 = 12.0;

static EMOJI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\p{Emoji_Presentation}|\p{Extended_Pictographic}").unwrap());
static FORMATTING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</?(?:b|strong|i|em|u|s|del)\s*>").unwrap());
static BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static STRIPPED_TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</?(?:sub|sup|small|mark)\s*>").unwrap());
static BOLD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*([^\n\r\x{2028}\x{2029}]+?)\*\*").unwrap());
static ITALIC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*([^\s*](?:[^*]*[^\s*])?)\*").unwrap());
static STRIKE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"~~([^\n\r\x{2028}\x{2029}]+?)~~").unwrap());

/// Normalize Mermaid quoting, line breaks, and upstream's supported Markdown.
/// Other HTML remains literal text and must be escaped by the SVG writer.
pub fn normalize_br_tags(label: &str) -> String {
    let unquoted = if label.starts_with('"') && label.ends_with('"') {
        // JavaScript slice(1, -1) also returns empty for a single quote.
        label.get(1..label.len().saturating_sub(1)).unwrap_or("")
    } else {
        label
    };
    let text = BREAK.replace_all(unquoted, "\n").replace("\\n", "\n");
    let text = STRIPPED_TAGS.replace_all(&text, "");
    let text = BOLD.replace_all(&text, "<b>$1</b>");
    // Rust regex has no look-around. Preserve upstream's negative-star guards
    // around the italic match without consuming neighboring characters.
    let mut italic = String::with_capacity(text.len());
    let mut previous = 0;
    let mut search = 0;
    while let Some(captures) = ITALIC.captures_at(&text, search) {
        let matched = captures.get(0).unwrap();
        if text.as_bytes().get(matched.start().wrapping_sub(1)) == Some(&b'*')
            || text.as_bytes().get(matched.end()) == Some(&b'*')
        {
            search = matched.start() + 1;
            continue;
        }
        italic.push_str(&text[previous..matched.start()]);
        italic.push_str("<i>");
        italic.push_str(&captures[1]);
        italic.push_str("</i>");
        previous = matched.end();
        search = matched.end();
    }
    italic.push_str(&text[previous..]);
    STRIKE.replace_all(&italic, "<s>$1</s>").into_owned()
}

/// Remove only formatting tags recognized by the upstream text renderer.
pub fn strip_formatting_tags(text: &str) -> String {
    FORMATTING.replace_all(text, "").into_owned()
}

fn is_combining_mark(code: u32) -> bool {
    matches!(code, 0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff |
        0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

fn is_fullwidth(code: u32) -> bool {
    matches!(code, 0x1100..=0x115f | 0x2e80..=0x2eff | 0x2f00..=0x2fdf |
        0x3000..=0x303f | 0x3040..=0x309f | 0x30a0..=0x30ff | 0x3100..=0x312f |
        0x3130..=0x318f | 0x3190..=0x31ff | 0x3200..=0x33ff | 0x3400..=0x4dbf |
        0x4e00..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xff00..=0xff60 |
        0xffe0..=0xffe6 | 0x20000..)
}

/// Relative glyph width using the same precedence and buckets as upstream.
pub fn char_width(character: char) -> f64 {
    let code = character as u32;
    if is_combining_mark(code) {
        return 0.0;
    }
    let mut encoded = [0; 4];
    if is_fullwidth(code) || EMOJI.is_match(character.encode_utf8(&mut encoded)) {
        return 2.0;
    }
    match character {
        ' ' => 0.3,
        'W' | 'M' => 1.5,
        'w' | 'm' | '@' | '%' => 1.2,
        'i' | 'l' | 't' | 'f' | 'j' | 'I' | '1' | '!' | '|' | '.' | ',' | ':' | ';' | '\'' => 0.4,
        '(' | ')' | '[' | ']' | '{' | '}' | '/' | '\\' | '-' | '"' | '`' => 0.5,
        'r' => 0.8,
        'A'..='Z' => 1.2,
        _ => 1.0,
    }
}

/// Estimate text width in pixels, including upstream's end-of-line buffer.
pub fn measure_text_width(text: &str, font_size: f64, font_weight: u16) -> f64 {
    let base_ratio = if font_weight >= 600 {
        0.60
    } else if font_weight >= 500 {
        0.57
    } else {
        0.54
    };
    text.chars().map(char_width).sum::<f64>() * font_size * base_ratio + font_size * 0.15
}

#[derive(Clone, Debug, PartialEq)]
pub struct Metrics {
    pub width: f64,
    pub height: f64,
    pub lines: Vec<String>,
    pub line_height: f64,
}

/// Measure all lines, preserving blank and trailing lines exactly as JS split.
pub fn measure_multiline_text(text: &str, font_size: f64, font_weight: u16) -> Metrics {
    let lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let line_height = font_size * LINE_HEIGHT_RATIO;
    let width = lines
        .iter()
        .map(|line| measure_text_width(&strip_formatting_tags(line), font_size, font_weight))
        .fold(0.0, f64::max);
    Metrics {
        width,
        height: lines.len() as f64 * line_height,
        lines,
        line_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
    }
    #[test]
    fn preserves_upstream_character_precedence() {
        close(char_width('W'), 1.5);
        close(char_width('1'), 0.4);
        close(char_width('8'), 1.0);
        close(char_width('#'), 1.0);
        close(char_width('©'), 2.0);
        close(char_width('🙂'), 2.0);
        close(char_width('中'), 2.0);
        close(char_width('\u{20000}'), 2.0);
        close(char_width('\u{0301}'), 0.0);
        // Match the upstream approximation, including its nonzero ZWJ width.
        close(char_width('\u{200d}'), 1.0);
    }
    #[test]
    fn preserves_empty_and_trailing_lines() {
        let metrics = measure_multiline_text("A\n", 13.0, 500);
        assert_eq!(metrics.lines, ["A", ""]);
        close(metrics.height, 33.8);
        close(metrics.width, 10.842);
        let empty = measure_multiline_text("", 13.0, 500);
        close(empty.width, 1.95);
        close(empty.height, 16.9);
    }
    #[test]
    fn normalizes_breaks_and_inline_formats() {
        assert_eq!(
            normalize_br_tags("\"One<BR />Two\\nThree\""),
            "One\nTwo\nThree"
        );
        assert_eq!(
            normalize_br_tags("**Bold** *italic* ~~gone~~"),
            "<b>Bold</b> <i>italic</i> <s>gone</s>"
        );
        assert_eq!(
            normalize_br_tags("<sub>x</sub><sup>2</sup><small>text</small><mark>mark</mark>"),
            "x2textmark"
        );
        assert_eq!(normalize_br_tags("* bad* *bad *"), "* bad* *bad *");
        assert_eq!(
            normalize_br_tags("<script>text</script>"),
            "<script>text</script>"
        );
    }
    #[test]
    fn upstream_oracle_metrics_match() {
        // Expected values generated directly by Beautiful Mermaid 1.1.3.
        close(
            measure_multiline_text("Gate 0 交付形态\n2 周 / 10 家访谈", 13.0, 500).width,
            99.762,
        );
        close(
            measure_multiline_text("Wim 123 ©🙂 e\u{0301}", 13.0, 500).width,
            86.424,
        );
        close(
            measure_multiline_text("<b>Mixed</b> <i>text</i>", 13.0, 500).width,
            61.23,
        );
        assert_eq!(normalize_br_tags("***abc***"), "<b><i>abc</b></i>");
        assert_eq!(normalize_br_tags("**a\nb**"), "**a\nb**");
    }
    #[test]
    fn formatting_tags_do_not_change_measurement() {
        close(
            measure_multiline_text("<b>Mixed</b> <i>text</i>", 13.0, 500).width,
            measure_multiline_text("Mixed text", 13.0, 500).width,
        );
        close(
            measure_text_width("e\u{0301}", 13.0, 500),
            measure_text_width("e", 13.0, 500),
        );
    }
}
