//! Native XY chart port from Beautiful Mermaid 1.1.3, Copyright (c) 2026 Craft Docs.
//! MIT; see licenses/Beautiful-Mermaid-MIT.txt. Static SVG only.
use crate::{RenderOptions, metrics::measure_text_width, render::escape, theme};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{fmt::Write, sync::LazyLock};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Axis {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Series {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: Vec<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub horizontal: bool,
    pub x_axis: Axis,
    pub y_axis: Axis,
    pub series: Vec<Series>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Title {
    pub text: String,
    pub x: f64,
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotate: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tick {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub tx: f64,
    pub ty: f64,
    pub label_x: f64,
    pub label_y: f64,
    pub text_anchor: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionedAxis {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Title>,
    pub ticks: Vec<Tick>,
    pub line: Segment,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bar {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub value: f64,
    pub label: String,
    pub series_index: usize,
    pub color_index: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
    pub value: f64,
    pub label: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub points: Vec<DataPoint>,
    pub series_index: usize,
    pub color_index: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Legend {
    pub label: String,
    pub x: f64,
    pub y: f64,
    #[serde(rename = "type")]
    pub kind: String,
    pub series_index: usize,
    pub color_index: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionedChart {
    pub width: f64,
    pub height: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Title>,
    pub x_axis: PositionedAxis,
    pub y_axis: PositionedAxis,
    pub plot_area: Rect,
    pub bars: Vec<Bar>,
    pub lines: Vec<Line>,
    pub grid_lines: Vec<Segment>,
    pub legend: Vec<Legend>,
}

static HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^xychart(?:-beta)?(?:\s+(horizontal))?$").unwrap());
static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^title\s+"([^"]+)"$"#).unwrap());
static CATEGORIES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^x-axis\s+(?:"([^"]*)"\s*)?\[([^\]]+)\]$"#).unwrap());
static RANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^([xy])-axis\s+(?:"([^"]*)"\s+)?(-?\d+(?:\.\d+)?)\s*-->\s*(-?\d+(?:\.\d+)?)$"#)
        .unwrap()
});
static Y_TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^y-axis\s+"([^"]+)"$"#).unwrap());
static SERIES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(bar|line)\s+\[([^\]]+)\]$").unwrap());

/// Parse the pinned upstream XY grammar with strict data validation.
pub fn parse(source: &str) -> Result<Chart, String> {
    if source.len() > 65_536 {
        return Err("XY chart exceeds 64 KiB input limit".into());
    }
    let mut lines = source
        .lines()
        .enumerate()
        .map(|(i, l)| (i, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with("%%"));
    let (_, header) = lines.next().ok_or("Empty XY chart")?;
    let cap = HEADER
        .captures(header)
        .ok_or("Expected xychart[-beta] [horizontal]")?;
    let mut chart = Chart {
        title: None,
        horizontal: cap.get(1).is_some(),
        x_axis: Axis::default(),
        y_axis: Axis::default(),
        series: vec![],
    };
    for (i, line) in lines {
        if let Some(c) = TITLE.captures(line) {
            chart.title = Some(c[1].into());
            continue;
        }
        if let Some(c) = CATEGORIES.captures(line) {
            if chart.x_axis.range.is_some() || chart.x_axis.categories.is_some() {
                return Err("Duplicate x-axis definition".into());
            }
            let categories: Vec<String> = c[2].split(',').map(|s| s.trim().to_owned()).collect();
            if categories.iter().any(String::is_empty) {
                return Err("Empty category label".into());
            }
            chart.x_axis.categories = Some(categories);
            chart.x_axis.title = c
                .get(1)
                .filter(|m| !m.as_str().is_empty())
                .map(|m| m.as_str().into());
            continue;
        }
        if let Some(c) = RANGE.captures(line) {
            let min = c[3].parse::<f64>().map_err(|_| "Invalid axis minimum")?;
            let max = c[4].parse::<f64>().map_err(|_| "Invalid axis maximum")?;
            if !min.is_finite() || !max.is_finite() || max <= min || !(max - min).is_finite() {
                return Err("Axis range must be finite and increasing".into());
            }
            let axis = if &c[1] == "x" {
                &mut chart.x_axis
            } else {
                &mut chart.y_axis
            };
            if axis.range.is_some() || axis.categories.is_some() {
                return Err("Duplicate axis definition".into());
            }
            axis.range = Some(Range { min, max });
            axis.title = c
                .get(2)
                .filter(|m| !m.as_str().is_empty())
                .map(|m| m.as_str().into());
            continue;
        }
        if let Some(c) = Y_TITLE.captures(line) {
            chart.y_axis.title = Some(c[1].into());
            continue;
        }
        if let Some(c) = SERIES.captures(line) {
            let data: Vec<f64> = c[2]
                .split(',')
                .map(|s| {
                    s.trim()
                        .parse::<f64>()
                        .map_err(|_| "Invalid numeric series value".to_owned())
                })
                .collect::<Result<_, _>>()?;
            if data.is_empty()
                || data.len() > 1000
                || data.iter().any(|v| !v.is_finite() || v.abs() > 1e100)
            {
                return Err("Series requires 1–1000 finite values within ±1e100".into());
            }
            if chart.series.len() >= 32 {
                return Err("At most 32 series are supported".into());
            }
            chart.series.push(Series {
                kind: c[1].into(),
                data,
            });
            continue;
        }
        return Err(format!("Line {}: unsupported XY chart syntax", i + 1));
    }
    if chart.series.is_empty() {
        return Err("XY chart requires at least one series".into());
    }
    let count = chart
        .x_axis
        .categories
        .as_ref()
        .map_or(chart.series[0].data.len(), Vec::len);
    if chart.series.iter().any(|s| s.data.len() != count) {
        return Err("Every series must match the category or first-series length".into());
    }
    if chart.y_axis.range.is_none() {
        let min = chart
            .series
            .iter()
            .flat_map(|s| s.data.iter())
            .copied()
            .fold(f64::INFINITY, f64::min);
        let max = chart
            .series
            .iter()
            .flat_map(|s| s.data.iter())
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let span = if max == min { 1.0 } else { max - min };
        let mut low = min - span * 0.1;
        let high = max + span * 0.1;
        if low > 0.0 && low < span * 0.5 {
            low = 0.0;
        }
        chart.y_axis.range = Some(Range {
            min: low,
            max: high,
        });
    }
    let r = chart.y_axis.range.as_ref().unwrap();
    if !r.min.is_finite()
        || !r.max.is_finite()
        || r.max <= r.min
        || r.min.abs() > 1e100
        || r.max.abs() > 1e100
    {
        return Err("XY axis magnitude or span is not representable".into());
    }
    // Preserve faithful layout for valid data; reject values that would draw outside the chart.
    if chart
        .series
        .iter()
        .flat_map(|s| &s.data)
        .any(|v| *v < r.min || *v > r.max)
    {
        return Err("Series value is outside the explicit y-axis range".into());
    }
    Ok(chart)
}
fn format_tick(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else if v.abs() < 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.0}")
    }
}
fn nice_ticks(min: f64, max: f64) -> Vec<f64> {
    let raw = (max - min) / 6.0;
    if raw <= 0.0 {
        return vec![min];
    }
    let magnitude = 10_f64.powf(raw.log10().floor());
    let residual = raw / magnitude;
    let interval = if residual <= 1.5 {
        magnitude
    } else if residual <= 3.0 {
        2.0 * magnitude
    } else if residual <= 7.0 {
        5.0 * magnitude
    } else {
        10.0 * magnitude
    };
    if interval == 0.0 || !interval.is_finite() {
        return vec![min, max];
    }
    let start = (min / interval).ceil() * interval;
    let mut result = vec![];
    for i in 0..100 {
        let v = start + i as f64 * interval;
        if v > max + interval * 0.001 {
            break;
        }
        let rounded = if interval < 1e-10 {
            v
        } else {
            (v * 1e10).round() / 1e10
        };
        result.push(if rounded == 0.0 { 0.0 } else { rounded });
    }
    if result.is_empty() {
        result.push(min);
    }
    result
}
fn labels(chart: &Chart) -> Vec<String> {
    let count = chart.series[0].data.len();
    if let Some(labels) = &chart.x_axis.categories {
        return labels.clone();
    }
    if let Some(r) = &chart.x_axis.range {
        let step = if count > 1 {
            (r.max - r.min) / (count - 1) as f64
        } else {
            0.0
        };
        return (0..count)
            .map(|i| format_tick(r.min + step * i as f64))
            .collect();
    }
    (1..=count).map(|i| i.to_string()).collect()
}
fn title(text: &Option<String>, x: f64, y: f64, rotate: Option<f64>) -> Option<Title> {
    text.as_ref().map(|text| Title {
        text: text.clone(),
        x,
        y,
        rotate,
    })
}

/// Compute the same fixed-size plotting area and upstream chart coordinates.
pub fn layout(chart: &Chart) -> Result<PositionedChart, String> {
    if chart.series.is_empty() {
        return Err("XY chart requires series".into());
    }
    let range = chart.y_axis.range.as_ref().ok_or("Missing y-axis range")?;
    let cat = labels(chart);
    if cat.is_empty() || !range.min.is_finite() || !range.max.is_finite() || range.max <= range.min
    {
        return Err("Invalid XY axis".into());
    }
    let ticks = nice_ticks(range.min, range.max);
    let count = cat.len();
    let horizontal = chart.horizontal;
    let top = 22.0
        + if chart.title.is_some() { 42.0 } else { 0.0 }
        + if chart.series.len() > 1 { 28.0 } else { 0.0 };
    let left_label = if horizontal {
        cat.iter()
            .map(|s| measure_text_width(s, 14.0, 400))
            .fold(40.0, f64::max)
    } else {
        ticks
            .iter()
            .map(|v| measure_text_width(&format_tick(*v), 14.0, 400))
            .fold(58.0, f64::max)
    };
    let left = 22.0
        + left_label
        + 18.0
        + if if horizontal {
            chart.x_axis.title.is_some()
        } else {
            chart.y_axis.title.is_some()
        } {
            30.0
        } else {
            0.0
        };
    let bottom = 60.0
        + if if horizontal {
            chart.y_axis.title.is_some()
        } else {
            chart.x_axis.title.is_some()
        } {
            30.0
        } else {
            0.0
        };
    let width = left + 600.0 + 22.0;
    let height = top + 340.0 + bottom;
    let band = if horizontal { 340.0 } else { 600.0 } / count as f64;
    let cat_scale = |i: usize| if horizontal { top } else { left } + (i as f64 + 0.5) * band;
    let value_scale = |v: f64| {
        let t = (v - range.min) / (range.max - range.min);
        if horizontal {
            left + t * 600.0
        } else {
            top + 340.0 - t * 340.0
        }
    };
    let cat_ticks: Vec<Tick> = cat
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let c = cat_scale(i);
            if horizontal {
                Tick {
                    label: label.clone(),
                    x: left,
                    y: c,
                    tx: left - 4.0,
                    ty: c,
                    label_x: left - 18.0,
                    label_y: c,
                    text_anchor: "end".into(),
                }
            } else {
                Tick {
                    label: label.clone(),
                    x: c,
                    y: top + 340.0,
                    tx: c,
                    ty: top + 344.0,
                    label_x: c,
                    label_y: top + 358.0,
                    text_anchor: "middle".into(),
                }
            }
        })
        .collect();
    let value_ticks: Vec<Tick> = ticks
        .iter()
        .map(|v| {
            let c = value_scale(*v);
            if horizontal {
                Tick {
                    label: format_tick(*v),
                    x: c,
                    y: top + 340.0,
                    tx: c,
                    ty: top + 344.0,
                    label_x: c,
                    label_y: top + 358.0,
                    text_anchor: "middle".into(),
                }
            } else {
                Tick {
                    label: format_tick(*v),
                    x: left,
                    y: c,
                    tx: left - 4.0,
                    ty: c,
                    label_x: left - 18.0,
                    label_y: c,
                    text_anchor: "end".into(),
                }
            }
        })
        .collect();
    let grid_lines = ticks
        .iter()
        .map(|v| {
            let c = value_scale(*v);
            if horizontal {
                Segment {
                    x1: c,
                    y1: top,
                    x2: c,
                    y2: top + 340.0,
                }
            } else {
                Segment {
                    x1: left,
                    y1: c,
                    x2: left + 600.0,
                    y2: c,
                }
            }
        })
        .collect();
    let (x_ticks, y_ticks) = if horizontal {
        (value_ticks, cat_ticks)
    } else {
        (cat_ticks, value_ticks)
    };
    let x_axis = PositionedAxis {
        ticks: x_ticks,
        line: Segment {
            x1: left,
            y1: top + 340.0,
            x2: left + 600.0,
            y2: top + 340.0,
        },
        title: title(
            if horizontal {
                &chart.y_axis.title
            } else {
                &chart.x_axis.title
            },
            left + 300.0,
            height - 22.0,
            None,
        ),
    };
    let y_axis = PositionedAxis {
        ticks: y_ticks,
        line: Segment {
            x1: left,
            y1: top,
            x2: left,
            y2: top + 340.0,
        },
        title: title(
            if horizontal {
                &chart.x_axis.title
            } else {
                &chart.y_axis.title
            },
            26.0,
            top + 170.0,
            Some(-90.0),
        ),
    };
    let bar_count = chart.series.iter().filter(|s| s.kind == "bar").count();
    let bar_width = if bar_count > 0 {
        (band * 0.8 / bar_count as f64).min(40.0)
    } else {
        0.0
    };
    let mut bars = vec![];
    let mut lines = vec![];
    let mut legend = vec![];
    let mut bar_index = 0;
    let mut line_index = 0;
    for (index, series) in chart.series.iter().enumerate() {
        let type_index = if series.kind == "bar" {
            bar_index
        } else {
            line_index
        };
        if series.data.len() != count || series.data.iter().any(|v| !v.is_finite()) {
            return Err("Invalid XY series length or value".into());
        }
        if series.kind == "bar" {
            for (i, v) in series.data.iter().enumerate() {
                let c = cat_scale(i) - bar_width * bar_count as f64 / 2.0
                    + bar_index as f64 * bar_width;
                let base = value_scale(0.0_f64.clamp(range.min, range.max));
                let val = value_scale(if horizontal { v.max(range.min) } else { *v });
                bars.push(if horizontal {
                    Bar {
                        x: base.min(val),
                        y: c,
                        width: (val - base).abs(),
                        height: bar_width,
                        value: *v,
                        label: cat[i].clone(),
                        series_index: bar_index,
                        color_index: index,
                    }
                } else {
                    Bar {
                        x: c,
                        y: base.min(val),
                        width: bar_width,
                        height: (val - base).abs(),
                        value: *v,
                        label: cat[i].clone(),
                        series_index: bar_index,
                        color_index: index,
                    }
                });
            }
            bar_index += 1;
        } else if series.kind == "line" {
            lines.push(Line {
                points: series
                    .data
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let c = cat_scale(i);
                        let val = value_scale(*v);
                        DataPoint {
                            x: if horizontal { val } else { c },
                            y: if horizontal { c } else { val },
                            value: *v,
                            label: cat[i].clone(),
                        }
                    })
                    .collect(),
                series_index: line_index,
                color_index: index,
            });
            line_index += 1;
        } else {
            return Err("Unsupported XY series type".into());
        }
        if chart.series.len() > 1 {
            legend.push(Legend {
                label: format!(
                    "{} {}",
                    if series.kind == "bar" { "Bar" } else { "Line" },
                    type_index + 1
                ),
                x: 0.0,
                y: 22.0 + if chart.title.is_some() { 42.0 } else { 0.0 } + 14.0,
                kind: series.kind.clone(),
                series_index: type_index,
                color_index: index,
            });
        }
    }
    let widths: Vec<f64> = legend
        .iter()
        .map(|l| 20.0 + measure_text_width(&l.label, 14.0, 400))
        .collect();
    let total = widths.iter().sum::<f64>() + legend.len().saturating_sub(1) as f64 * 16.0;
    let mut x = width / 2.0 - total / 2.0;
    for (item, w) in legend.iter_mut().zip(widths) {
        item.x = x;
        x += w + 16.0;
    }
    Ok(PositionedChart {
        width,
        height,
        horizontal: if horizontal { Some(true) } else { None },
        title: title(&chart.title, width / 2.0, 40.0, None),
        x_axis,
        y_axis,
        plot_area: Rect {
            x: left,
            y: top,
            width: 600.0,
            height: 340.0,
        },
        bars,
        lines,
        grid_lines,
        legend,
    })
}
fn rgb(color: &str) -> Result<[f64; 3], String> {
    let c = theme::color(color)?;
    if c.len() != 7 || !c.starts_with('#') {
        return Err("XY chart colors require RGB values".into());
    }
    Ok([1, 3, 5].map(|i| u8::from_str_radix(&c[i..i + 2], 16).unwrap() as f64))
}
fn hex(c: [f64; 3]) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        c[0].round().clamp(0.0, 255.0) as u8,
        c[1].round().clamp(0.0, 255.0) as u8,
        c[2].round().clamp(0.0, 255.0) as u8
    )
}
fn mix(bg: &str, fg: &str, ratio: f64) -> Result<String, String> {
    let b = rgb(bg)?;
    let f = rgb(fg)?;
    Ok(hex([0, 1, 2].map(|i| b[i] * (1.0 - ratio) + f[i] * ratio)))
}
fn hsl(color: &str) -> Result<[f64; 3], String> {
    let [r, g, b] = rgb(color)?.map(|v| v / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return Ok([0.0, 0.0, l * 100.0]);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let hue = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    Ok([hue * 360.0, s * 100.0, l * 100.0])
}
fn hsl_hex(h: f64, s: f64, l: f64) -> String {
    let s = s / 100.0;
    let l = l / 100.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let v = if h < 60.0 {
        [c, x, 0.0]
    } else if h < 120.0 {
        [x, c, 0.0]
    } else if h < 180.0 {
        [0.0, c, x]
    } else if h < 240.0 {
        [0.0, x, c]
    } else if h < 300.0 {
        [x, 0.0, c]
    } else {
        [c, 0.0, x]
    };
    hex(v.map(|a| (a + m) * 255.0))
}
fn series_color(index: usize, accent: &str, bg: &str) -> Result<String, String> {
    if index == 0 {
        return theme::color(accent);
    }
    let [h, s, _] = hsl(accent)?;
    let tier = index.div_ceil(2) as f64;
    let odd = index % 2 == 1;
    let dark = if hsl(bg)?[2] < 50.0 { !odd } else { odd };
    let l = if dark {
        (48.0 - tier * 13.0).max(25.0)
    } else {
        (55.0 + tier * 11.0).min(78.0)
    };
    let shifted = (h + if dark { -8.0 * tier } else { 12.0 * tier }).rem_euclid(360.0);
    Ok(hsl_hex(shifted, s.clamp(55.0, 85.0), l))
}
fn r(value: f64) -> String {
    let v = (value * 10.0 + 0.5).floor() / 10.0;
    if v == 0.0 { "0".into() } else { v.to_string() }
}
fn rounded_bar(bar: &Bar, horizontal: bool) -> String {
    let (x, y, w, h) = (bar.x, bar.y, bar.width, bar.height);
    let rr = 8.0_f64.min(w / 2.0).min(h / 2.0);
    if rr <= 0.0 {
        return format!("M{},{} h{} v{} h{} Z", r(x), r(y), r(w), r(h), r(-w));
    }
    if horizontal {
        format!(
            "M{},{} L{},{} Q{},{} {},{} L{},{} Q{},{} {},{} L{},{} Q{},{} {},{} L{},{} Q{},{} {},{} Z",
            r(x + rr),
            r(y),
            r(x + w - rr),
            r(y),
            r(x + w),
            r(y),
            r(x + w),
            r(y + rr),
            r(x + w),
            r(y + h - rr),
            r(x + w),
            r(y + h),
            r(x + w - rr),
            r(y + h),
            r(x + rr),
            r(y + h),
            r(x),
            r(y + h),
            r(x),
            r(y + h - rr),
            r(x),
            r(y + rr),
            r(x),
            r(y),
            r(x + rr),
            r(y)
        )
    } else {
        format!(
            "M{},{} Q{},{} {},{} L{},{} Q{},{} {},{} L{},{} Q{},{} {},{} L{},{} Q{},{} {},{} Z",
            r(x),
            r(y + rr),
            r(x),
            r(y),
            r(x + rr),
            r(y),
            r(x + w - rr),
            r(y),
            r(x + w),
            r(y),
            r(x + w),
            r(y + rr),
            r(x + w),
            r(y + h - rr),
            r(x + w),
            r(y + h),
            r(x + w - rr),
            r(y + h),
            r(x + rr),
            r(y + h),
            r(x),
            r(y + h),
            r(x),
            r(y + h - rr)
        )
    }
}
fn curve(points: &[DataPoint], horizontal: bool) -> String {
    let n = points.len();
    if n == 0 {
        return String::new();
    }
    let mut path = format!("M{},{}", r(points[0].x), r(points[0].y));
    if n == 1 {
        return path;
    }
    if n == 2 {
        return format!("{path} L{},{}", r(points[1].x), r(points[1].y));
    }
    // Interpolate along category order. The upstream x-only spline is singular
    // for horizontal charts with repeated or nonmonotonic values.
    let coordinate = |p: &DataPoint| if horizontal { (p.y, p.x) } else { (p.x, p.y) };
    let mut h = vec![0.0; n - 1];
    let mut delta = vec![0.0; n - 1];
    for i in 0..n - 1 {
        let (a, b) = coordinate(&points[i]);
        let (c, d) = coordinate(&points[i + 1]);
        h[i] = c - a;
        delta[i] = (d - b) / h[i];
    }
    let mut c = vec![0.0; n];
    let mut cp = vec![0.0; n];
    let mut dp = vec![0.0; n];
    for i in 1..n - 1 {
        let diag = 2.0 * (h[i - 1] + h[i]);
        let rhs = 3.0 * (delta[i] - delta[i - 1]);
        let w = if i == 1 {
            diag
        } else {
            diag - h[i - 1] * cp[i - 1]
        };
        cp[i] = h[i] / w;
        dp[i] = if i == 1 {
            rhs / w
        } else {
            (rhs - h[i - 1] * dp[i - 1]) / w
        };
    }
    for i in (1..n - 1).rev() {
        c[i] = dp[i] - cp[i] * c[i + 1];
    }
    let mut slopes = vec![0.0; n];
    for i in 0..n - 1 {
        slopes[i] = delta[i] - h[i] * (2.0 * c[i] + c[i + 1]) / 3.0;
    }
    slopes[n - 1] = delta[n - 2] + h[n - 2] * c[n - 2] / 3.0;
    for i in 0..n - 1 {
        let seg = h[i] / 3.0;
        let (x1, y1) = coordinate(&points[i]);
        let (x2, y2) = coordinate(&points[i + 1]);
        let a = (x1 + seg, y1 + slopes[i] * seg);
        let b = (x2 - seg, y2 - slopes[i + 1] * seg);
        let (a, b) = if horizontal {
            ((a.1, a.0), (b.1, b.0))
        } else {
            (a, b)
        };
        write!(
            path,
            " C{},{} {},{} {},{}",
            r(a.0),
            r(a.1),
            r(b.0),
            r(b.1),
            r(points[i + 1].x),
            r(points[i + 1].y)
        )
        .unwrap();
    }
    path
}
#[allow(clippy::too_many_arguments)]
fn text_at(
    text: &str,
    x: f64,
    y: f64,
    size: u16,
    weight: u16,
    color: &str,
    anchor: &str,
    rotate: Option<f64>,
) -> String {
    let rotation = rotate
        .map(|a| format!(r#" transform="rotate({a},{x},{y})""#))
        .unwrap_or_default();
    format!(
        r#"<text x="{x}" y="{y}" text-anchor="{anchor}"{rotation} font-size="{size}" font-weight="{weight}" dy="0.35em" fill="{color}">{}</text>"#,
        escape(text)
    )
}
/// Render a static XY chart using the same native theme color resolution.
pub fn render(source: &str, options: &RenderOptions) -> Result<String, String> {
    let chart = layout(&parse(source)?)?;
    render_positioned(&chart, options)
}
pub fn render_positioned(
    chart: &PositionedChart,
    options: &RenderOptions,
) -> Result<String, String> {
    let p = theme::resolve(&options.colors)?;
    let muted = if let Some(c) = &options.colors.muted {
        theme::color(c)?
    } else {
        mix(&p.bg, &p.text, 0.4)?
    };
    let accent = options.colors.accent.as_deref().unwrap_or("#3b82f6");
    let max_index = chart
        .bars
        .iter()
        .map(|b| b.color_index)
        .chain(chart.lines.iter().map(|l| l.color_index))
        .max()
        .unwrap_or(0);
    let colors: Vec<String> = (0..=max_index)
        .map(|i| series_color(i, accent, &p.bg))
        .collect::<Result<_, _>>()?;
    let fills: Vec<String> = colors
        .iter()
        .map(|c| mix(&p.bg, c, 0.25))
        .collect::<Result<_, _>>()?;
    let mut out = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" data-xychart-colors="{max_index}" width="{}" height="{}" viewBox="0 0 {} {}" font-family="{}, system-ui, sans-serif">"#,
        chart.width,
        chart.height,
        chart.width,
        chart.height,
        escape(&options.font)
    );
    if !options.transparent {
        write!(out, r#"<rect width="100%" height="100%" fill="{}"/>"#, p.bg).unwrap();
    }
    let horizontal = chart.horizontal.unwrap_or(false);
    let plot = &chart.plot_area;
    let xs: Vec<f64> = chart.x_axis.ticks.iter().map(|t| t.x).collect();
    let ys: Vec<f64> = if horizontal {
        chart.y_axis.ticks.iter().map(|t| t.y).collect()
    } else {
        chart.grid_lines.iter().map(|l| l.y1).collect()
    };
    let x_base = if xs.len() > 1 {
        (xs[1] - xs[0]).abs()
    } else {
        plot.width / 6.0
    };
    let y_base = if ys.len() > 1 {
        (ys[1] - ys[0]).abs()
    } else {
        plot.height / 6.0
    };
    let x_gap = x_base / (x_base / 20.0).round().max(1.0);
    let y_gap = y_base / (y_base / 20.0).round().max(1.0);
    if !x_gap.is_finite() || !y_gap.is_finite() || x_gap <= 0.0 || y_gap <= 0.0 {
        return Err("Invalid XY grid spacing".into());
    }
    let x_anchor = xs.first().copied().unwrap_or(plot.x);
    let y_anchor = ys.first().copied().unwrap_or(plot.y);
    let x_start = x_anchor - ((x_anchor - plot.x) / x_gap).ceil() * x_gap;
    let y_start = y_anchor - ((y_anchor - plot.y) / y_gap).ceil() * y_gap;
    let rows = ((plot.y + plot.height + 0.5 - y_start) / y_gap)
        .floor()
        .max(0.0) as usize
        + 1;
    let columns = ((plot.x + plot.width + 0.5 - x_start) / x_gap)
        .floor()
        .max(0.0) as usize
        + 1;
    if rows.checked_mul(columns).is_none_or(|n| n > 50_000) {
        return Err("XY grid exceeds rendering limit".into());
    }
    for iy in 0..rows {
        for ix in 0..columns {
            write!(
                out,
                r#"<circle cx="{}" cy="{}" r="1.5" fill="{}" stroke="none" opacity="0.65"/>"#,
                r(x_start + ix as f64 * x_gap),
                r(y_start + iy as f64 * y_gap),
                p.inner_stroke
            )
            .unwrap();
        }
    }
    for bar in &chart.bars {
        write!(out,r#"<path d="{}" class="xychart-bar xychart-color-{}" data-value="{}" data-label="{}" stroke="{}" fill="{}" stroke-width="1.5"/>"#,rounded_bar(bar,horizontal),bar.color_index,bar.value,escape(&bar.label),colors[bar.color_index],fills[bar.color_index]).unwrap();
    }
    for line in &chart.lines {
        if line.points.is_empty() {
            continue;
        }
        let d = curve(&line.points, horizontal);
        write!(out,r#"<path d="{d}" fill="none" stroke="{}" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" opacity="0.12" transform="translate(0,2)"/><path d="{d}" class="xychart-line xychart-color-{}" fill="none" stroke="{}" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>"#,colors[line.color_index],line.color_index,colors[line.color_index]).unwrap();
    }
    let max_points = chart
        .lines
        .iter()
        .map(|l| l.points.len())
        .max()
        .unwrap_or(0);
    if max_points > 0 && max_points <= 12 {
        for line in &chart.lines {
            for point in &line.points {
                write!(out,r#"<circle cx="{}" cy="{}" r="5" class="xychart-dot xychart-color-{}" data-value="{}" data-label="{}" fill="{}" stroke="{}" stroke-width="2"/>"#,r(point.x),r(point.y),line.color_index,point.value,escape(&point.label),colors[line.color_index],p.bg).unwrap();
            }
        }
    }
    for tick in chart.x_axis.ticks.iter().chain(&chart.y_axis.ticks) {
        out.push_str(&text_at(
            &tick.label,
            tick.label_x,
            tick.label_y,
            14,
            400,
            &muted,
            &tick.text_anchor,
            None,
        ));
    }
    for title in [&chart.x_axis.title, &chart.y_axis.title]
        .into_iter()
        .flatten()
    {
        out.push_str(&text_at(
            &title.text,
            title.x,
            title.y,
            15,
            500,
            &p.secondary,
            "middle",
            title.rotate,
        ));
    }
    if let Some(title) = &chart.title {
        out.push_str(&text_at(
            &title.text,
            title.x,
            title.y,
            18,
            600,
            &p.text,
            "middle",
            None,
        ));
    }
    for item in &chart.legend {
        if item.kind == "bar" {
            write!(out,r#"<rect x="{}" y="{}" width="14" height="14" rx="3" fill="{}" stroke="{}" stroke-width="1.5"/>"#,item.x,item.y-7.0,fills[item.color_index],colors[item.color_index]).unwrap();
        } else {
            write!(out,r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke-width="2.5" stroke-linecap="round" stroke="{}"/>"#,item.x,item.y,item.x+14.0,item.y,colors[item.color_index]).unwrap();
        }
        out.push_str(&text_at(
            &item.label,
            item.x + 20.0,
            item.y,
            14,
            400,
            &muted,
            "start",
            None,
        ));
    }
    out.push_str("</svg>");
    Ok(out)
}
