use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Compile MDX-like layout definitions into JSON trees."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile templates within the given directory into JSON layout trees
    Compile {
        /// Directory containing .mdx templates
        #[arg(default_value = "ui/screens")]
        input: PathBuf,
        /// Output directory for serialized layouts
        #[arg(default_value = "generated/layouts")]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Compile { input, output } => compile_templates(&input, &output),
    }
}

fn compile_templates(input: &PathBuf, output: &PathBuf) -> Result<()> {
    if !output.exists() {
        fs::create_dir_all(output)
            .with_context(|| format!("creating output directory {}", output.display()))?;
    }

    let mut compiled = 0usize;
    for entry in WalkDir::new(input).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("mdx") {
            continue;
        }
        let document = parse_template(entry.path())?;
        let target = output.join(format!("{}.json", document.meta.id));
        let json = serde_json::to_string_pretty(&document)?;
        fs::write(&target, json).with_context(|| format!("writing layout {}", target.display()))?;
        compiled += 1;
    }

    if compiled == 0 {
        bail!("no templates found in {}", input.display());
    }

    println!(
        "Compiled {compiled} layout templates into {}",
        output.display()
    );
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct LayoutDocument {
    meta: FrontMatter,
    nodes: Vec<LayoutNode>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LayoutNode {
    id: String,
    component: ComponentKind,
    layout: LayoutProps,
    accessibility: AccessibilityProps,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    children: Vec<LayoutNode>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LayoutProps {
    display: Display,
    flex_direction: FlexDirection,
    flex_grow: f32,
    padding: Edges,
    margin: Edges,
    z_index: i32,
}

impl Default for LayoutProps {
    fn default() -> Self {
        Self {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            padding: Edges::zero(),
            margin: Edges::zero(),
            z_index: 0,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct AccessibilityProps {
    label: Option<String>,
    tab_index: i32,
    mnemonic: Option<String>,
    role: Option<String>,
}

impl Default for AccessibilityProps {
    fn default() -> Self {
        Self {
            label: None,
            tab_index: 0,
            mnemonic: None,
            role: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ComponentKind {
    TerminalPane,
    OverlayRegion,
    MenuBar,
    Unknown,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Display {
    Flex,
    Grid,
    Absolute,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Deserialize, Serialize)]
struct Edges {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Edges {
    const fn zero() -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct FrontMatter {
    id: String,
    title: Option<String>,
}

fn parse_template(path: &std::path::Path) -> Result<LayoutDocument> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading template {}", path.display()))?;
    let (frontmatter, body) = split_frontmatter(&content)?;
    let meta: FrontMatter = serde_yaml::from_str(&frontmatter)
        .with_context(|| format!("invalid frontmatter in {}", path.display()))?;
    if meta.id.trim().is_empty() {
        bail!("template {} missing id", path.display());
    }

    let nodes = parse_components(&body)?;
    Ok(LayoutDocument { meta, nodes })
}

fn split_frontmatter(input: &str) -> Result<(String, String)> {
    let mut lines = input.lines();
    if lines.next() != Some("---") {
        bail!("template missing frontmatter start");
    }
    let mut front = String::new();
    let mut body_lines = Vec::new();
    let mut in_frontmatter = true;
    for line in input.lines().skip(1) {
        if in_frontmatter {
            if line.trim() == "---" {
                in_frontmatter = false;
                continue;
            }
            front.push_str(line);
            front.push('\n');
        } else {
            body_lines.push(line);
        }
    }
    if in_frontmatter {
        bail!("template missing closing frontmatter marker");
    }
    let body = body_lines.join("\n");
    Ok((front.trim_end().to_string(), body.trim().to_string()))
}

fn parse_components(body: &str) -> Result<Vec<LayoutNode>> {
    let tag_re = Regex::new(r"(?s)<\s*(/)?\s*([A-Za-z0-9_]+)([^<>]*)>").unwrap();
    let mut stack: Vec<NodeFrame> = Vec::new();
    let mut roots = Vec::new();
    let mut auto_index = 0usize;

    for cap in tag_re.captures_iter(body) {
        let is_close = cap.get(1).map(|m| !m.as_str().is_empty()).unwrap_or(false);
        let name = cap[2].trim();
        let mut raw_attrs = cap
            .get(3)
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let mut self_closing = false;
        if !is_close && raw_attrs.ends_with('/') {
            self_closing = true;
            raw_attrs = raw_attrs.trim_end_matches('/').trim().to_string();
        }

        if is_close {
            let frame = stack
                .pop()
                .ok_or_else(|| anyhow!("unexpected closing tag </{name}>"))?;
            if frame.name != name {
                bail!(
                    "mismatched closing tag </{name}> (expected </{}>)",
                    frame.name
                );
            }
            finalize_node(frame.node, &mut stack, &mut roots);
            continue;
        }

        let attrs = parse_attributes(&raw_attrs)?;
        let node = build_layout_node(name, &attrs, &mut auto_index);
        if self_closing {
            finalize_node(node, &mut stack, &mut roots);
        } else {
            stack.push(NodeFrame {
                name: name.to_string(),
                node,
            });
        }
    }

    if let Some(frame) = stack.pop() {
        bail!("unclosed component <{}>", frame.name);
    }

    if roots.is_empty() {
        bail!("template body contained no recognized components");
    }

    Ok(roots)
}

struct NodeFrame {
    name: String,
    node: LayoutNode,
}

fn finalize_node(node: LayoutNode, stack: &mut Vec<NodeFrame>, roots: &mut Vec<LayoutNode>) {
    if let Some(parent) = stack.last_mut() {
        parent.node.children.push(node);
    } else {
        roots.push(node);
    }
}

fn build_layout_node(
    name: &str,
    attrs: &HashMap<String, String>,
    auto_index: &mut usize,
) -> LayoutNode {
    let id = attrs.get("id").cloned().unwrap_or_else(|| {
        let generated = format!("{}_{}", name.to_lowercase(), *auto_index);
        *auto_index += 1;
        generated
    });
    let component = match name {
        "TerminalPane" => ComponentKind::TerminalPane,
        "OverlayRegion" => ComponentKind::OverlayRegion,
        "MenuBar" => ComponentKind::MenuBar,
        _ => ComponentKind::Unknown,
    };

    let mut layout = LayoutProps::default();
    if let Some(z) = attrs.get("z") {
        if let Ok(val) = z.parse::<i32>() {
            layout.z_index = val;
        }
    }

    let accessibility = AccessibilityProps {
        label: attrs.get("label").cloned(),
        tab_index: attrs
            .get("tabIndex")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0),
        mnemonic: attrs.get("mnemonic").cloned(),
        role: attrs.get("role").cloned(),
    };

    LayoutNode {
        id,
        component,
        layout,
        accessibility,
        children: Vec::new(),
    }
}

fn parse_attributes(input: &str) -> Result<HashMap<String, String>> {
    let attr_re =
        Regex::new(r#"([A-Za-z0-9_]+)\s*=\s*(?:\"([^\"]*)\"|([A-Za-z0-9_\.]+))"#).unwrap();
    let mut map = HashMap::new();
    for capture in attr_re.captures_iter(input) {
        let key = capture[1].to_string();
        let value = capture
            .get(2)
            .map(|m| m.as_str())
            .or_else(|| capture.get(3).map(|m| m.as_str()))
            .unwrap_or("")
            .to_string();
        map.insert(key, value);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_attributes() {
        let attrs = parse_attributes(r#"id="main" rows=40 label="Primary""#).unwrap();
        assert_eq!(attrs.get("id").unwrap(), "main");
        assert_eq!(attrs.get("rows").unwrap(), "40");
        assert_eq!(attrs.get("label").unwrap(), "Primary");
    }

    #[test]
    fn builds_nodes_from_body() {
        let body = "<TerminalPane id=\"main\" rows=40 />\n<OverlayRegion id=\"status\" z=900 />";
        let nodes = parse_components(body).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].id, "main");
        assert!(matches!(nodes[0].component, ComponentKind::TerminalPane));
        assert_eq!(nodes[1].layout.z_index, 900);
    }

    #[test]
    fn nested_nodes_build_tree() {
        let body = "\
            <MenuBar id=\"root\">\n\
              <TerminalPane id=\"child\" />\n\
              <OverlayRegion id=\"overlay\">\n\
                <TerminalPane />\n\
              </OverlayRegion>\n\
            </MenuBar>\n";
        let nodes = parse_components(body).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "root");
        assert_eq!(nodes[0].children.len(), 2);
        assert_eq!(nodes[0].children[0].id, "child");
        assert_eq!(nodes[0].children[1].children.len(), 1);
        assert!(nodes[0].children[1].children[0]
            .id
            .starts_with("terminalpane_"));
    }
}
