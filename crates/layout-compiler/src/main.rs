use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{bail, Context, Result};
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
    let component_re = Regex::new(r"<([A-Za-z0-9]+)\s+([^>/]+)/?>").unwrap();
    let mut nodes = Vec::new();
    for cap in component_re.captures_iter(body) {
        let name = &cap[1];
        let raw_attrs = &cap[2];
        let attrs = parse_attributes(raw_attrs)?;
        let id = attrs
            .get("id")
            .cloned()
            .unwrap_or_else(|| format!("{}_{}", name.to_lowercase(), nodes.len()));
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

        nodes.push(LayoutNode {
            id,
            component,
            layout,
            accessibility,
        });
    }

    if nodes.is_empty() {
        bail!("template body contained no recognized components");
    }

    Ok(nodes)
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
        matches!(nodes[0].component, ComponentKind::TerminalPane);
        assert_eq!(nodes[1].layout.z_index, 900);
    }
}
