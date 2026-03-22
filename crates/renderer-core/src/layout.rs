#![allow(dead_code)]
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LayoutDocument {
    pub meta: FrontMatter,
    pub nodes: Vec<LayoutNode>,
}

#[derive(Debug, Deserialize)]
pub struct FrontMatter {
    pub id: String,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LayoutNode {
    pub id: String,
    pub component: ComponentKind,
    pub layout: LayoutProps,
    pub accessibility: AccessibilityProps,
    #[serde(default)]
    pub children: Vec<LayoutNode>,
}

#[derive(Debug, Deserialize)]
pub struct LayoutProps {
    pub display: Display,
    pub flex_direction: FlexDirection,
    pub flex_grow: f32,
    pub padding: Edges,
    pub margin: Edges,
    pub z_index: i32,
}

#[derive(Debug, Deserialize)]
pub struct AccessibilityProps {
    pub label: Option<String>,
    pub tab_index: i32,
    pub mnemonic: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    TerminalPane,
    OverlayRegion,
    MenuBar,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Display {
    Flex,
    Grid,
    Absolute,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Deserialize)]
pub struct Edges {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}
