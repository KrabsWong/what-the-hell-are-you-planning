//! Shared graph types for the native flowchart renderer.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub type Style = BTreeMap<String, String>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub direction: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub subgraphs: Vec<Subgraph>,
    #[serde(default)]
    pub class_defs: BTreeMap<String, Style>,
    #[serde(default)]
    pub class_assignments: BTreeMap<String, String>,
    #[serde(default)]
    pub node_styles: BTreeMap<String, Style>,
    #[serde(default)]
    pub link_styles: BTreeMap<String, Style>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub label: Option<String>,
    pub style: String,
    pub has_arrow_start: bool,
    pub has_arrow_end: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subgraph {
    pub id: String,
    pub label: String,
    pub node_ids: Vec<String>,
    pub children: Vec<Subgraph>,
    pub direction: Option<String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionedGraph {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<PositionedNode>,
    pub edges: Vec<PositionedEdge>,
    pub groups: Vec<PositionedGroup>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionedNode {
    pub id: String,
    pub label: String,
    pub shape: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub inline_style: Style,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionedEdge {
    pub source: String,
    pub target: String,
    pub label: Option<String>,
    pub style: String,
    pub has_arrow_start: bool,
    pub has_arrow_end: bool,
    pub points: Vec<Point>,
    pub label_position: Option<Point>,
    #[serde(default)]
    pub inline_style: Style,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionedGroup {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub children: Vec<PositionedGroup>,
}
#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub font: String,
    pub padding: f64,
    pub node_spacing: f64,
    pub layer_spacing: f64,
    pub component_spacing: Option<f64>,
    pub transparent: bool,
    pub colors: DiagramColors,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            font: "Inter".into(),
            padding: 40.0,
            node_spacing: 28.0,
            layer_spacing: 48.0,
            component_spacing: None,
            transparent: false,
            colors: DiagramColors::default(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct DiagramColors {
    pub bg: String,
    pub fg: String,
    pub line: Option<String>,
    pub accent: Option<String>,
    pub muted: Option<String>,
    pub surface: Option<String>,
    pub border: Option<String>,
}
impl Default for DiagramColors {
    fn default() -> Self {
        Self {
            bg: "#FFFFFF".into(),
            fg: "#27272A".into(),
            line: None,
            accent: None,
            muted: None,
            surface: None,
            border: None,
        }
    }
}
