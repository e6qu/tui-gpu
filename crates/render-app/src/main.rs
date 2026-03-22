use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use env_logger;
use layout::{ComponentKind, LayoutDocument, LayoutNode};
use log::info;
use std::sync::Arc;
use taffy::prelude::{
    AvailableSpace, Dimension, FlexDirection as TaffyFlexDirection, LengthPercentageAuto, Node,
    Position, Rect as TaffyRect, Size, Style, Taffy,
};
use terminal_session::{Rgb as TermRgb, TerminalBufferSnapshot, TerminalSession};
use text::{GlyphVertex, TextPipeline, CELL_PIXEL_HEIGHT, CELL_PIXEL_WIDTH};
use wgpu::util::DeviceExt;
use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

mod layout;
mod text;

const MENU_BAR_HEIGHT: f32 = 48.0;
const OVERLAY_HEIGHT: f32 = 32.0;
const LAYOUT_PATH: &str = "generated/layouts/main_screen.json";

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum DemoKind {
    Terminal,
    Plasma,
    Ray,
    Doom,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum AppMode {
    Gui,
    Tui,
}

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long, value_enum, default_value_t = DemoKind::Terminal)]
    demo: DemoKind,
    #[arg(long, value_enum, default_value_t = AppMode::Gui)]
    mode: AppMode,
}

impl Cli {
    fn selected_mode(&self) -> AppMode {
        self.mode
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    match cli.selected_mode() {
        AppMode::Gui => run_gui(&cli),
        AppMode::Tui => run_tui(&cli),
    }
}

fn run_gui(_cli: &Cli) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("tui-gpu renderer")
            .build(&event_loop)?,
    );
    let mut state = pollster::block_on(State::new(window.clone()))?;
    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => target.exit(),
                WindowEvent::Resized(size) => state.resize(size),
                WindowEvent::ScaleFactorChanged { .. } => {
                    state.resize(window.inner_size());
                }
                WindowEvent::KeyboardInput { event, .. } => state.handle_key(event, target),
                WindowEvent::RedrawRequested => {
                    if let Err(err) = state.render() {
                        info!("render error: {err:?}");
                    }
                }
                _ => {}
            },
            Event::AboutToWait => window.request_redraw(),
            _ => {}
        }
    })?;
    Ok(())
}

fn run_tui(cli: &Cli) -> Result<()> {
    println!(
        "tui-gpu demo {:?} is not implemented in TUI mode yet. Use --mode gui.",
        cli.demo
    );
    Ok(())
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3];
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphInstance {
    inner: GlyphVertex,
}

struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    text_pipeline: TextPipeline,
    layout_engine: LayoutEngine,
    terminal: TerminalSession,
}

impl State {
    async fn new(window: Arc<winit::window::Window>) -> Result<Self> {
        let layout = load_layout()?;
        let mut layout_engine = LayoutEngine::new(&layout)?;
        let size = window.inner_size();
        let initial_rects = layout_engine.compute_layout(size.width as f32, size.height as f32)?;
        let terminal_rect = find_terminal_rect(&initial_rects)
            .unwrap_or(default_terminal_rect(size.width as f32, size.height as f32));
        let (terminal_cols, terminal_rows) = terminal_grid_size(terminal_rect);

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("request adapter")?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("render-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("grid.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid-pipeline-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Line,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let text_pipeline = TextPipeline::new(&device, &queue, config.format);
        let command = terminal_command();
        let terminal = TerminalSession::spawn(&command, terminal_cols, terminal_rows)?;

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            text_pipeline,
            layout_engine,
            terminal,
        })
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn handle_key(
        &mut self,
        event: KeyEvent,
        target: &winit::event_loop::EventLoopWindowTarget<()>,
    ) {
        if event.state == ElementState::Pressed {
            if let Some(text) = event.text.as_ref() {
                if !text.is_empty() {
                    let _ = self.terminal.write(text.as_bytes());
                    return;
                }
            }
            if let Some(seq) = special_key_sequence(&event) {
                let _ = self.terminal.write(&seq);
                return;
            }
            if matches!(
                event.logical_key,
                winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
            ) {
                target.exit();
            }
        }
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let window_width = self.config.width.max(1) as f32;
        let window_height = self.config.height.max(1) as f32;
        let rects = match self
            .layout_engine
            .compute_layout(window_width, window_height)
        {
            Ok(rects) => rects,
            Err(err) => {
                info!("layout compute error: {err:?}");
                return Ok(());
            }
        };
        let vertices = build_layout_vertices(&rects, window_width, window_height);
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("layout-vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let pane_rect = find_terminal_rect(&rects)
            .unwrap_or(default_terminal_rect(window_width, window_height));
        if let Err(err) = self.terminal.process_output() {
            info!("terminal read error: {err:?}");
        }
        let snapshot = self.terminal.snapshot();
        let glyph_vertices = build_terminal_vertices(
            &snapshot,
            pane_rect,
            window_width,
            window_height,
            self.text_pipeline.atlas(),
        );
        let glyph_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("glyph-vertices"),
                contents: bytemuck::cast_slice(&glyph_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.09,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);

            pass.set_pipeline(self.text_pipeline.pipeline());
            pass.set_bind_group(0, self.text_pipeline.bind_group(), &[]);
            pass.set_vertex_buffer(0, glyph_buffer.slice(..));
            pass.draw(0..glyph_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn build_layout_vertices(rects: &[(ComponentKind, Rect)], width: f32, height: f32) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    for (component, rect) in rects {
        let color = match component {
            ComponentKind::MenuBar => [0.3, 0.3, 0.35],
            ComponentKind::OverlayRegion => [0.25, 0.65, 0.25],
            ComponentKind::TerminalPane => [0.15, 0.35, 0.65],
            ComponentKind::Unknown => [0.2, 0.2, 0.2],
        };
        push_rect(&mut vertices, *rect, color, width, height);
    }
    vertices
}

#[derive(Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn find_terminal_rect(rects: &[(ComponentKind, Rect)]) -> Option<Rect> {
    rects.iter().find_map(|(component, rect)| {
        if matches!(component, ComponentKind::TerminalPane) {
            Some(*rect)
        } else {
            None
        }
    })
}

fn default_terminal_rect(width: f32, height: f32) -> Rect {
    let usable_height = (height - MENU_BAR_HEIGHT - OVERLAY_HEIGHT).max(0.0);
    Rect {
        x: 0.0,
        y: MENU_BAR_HEIGHT,
        width,
        height: usable_height,
    }
}

fn terminal_grid_size(rect: Rect) -> (usize, usize) {
    let cols = (rect.width / CELL_PIXEL_WIDTH).floor().max(1.0) as usize;
    let rows = (rect.height / CELL_PIXEL_HEIGHT).floor().max(1.0) as usize;
    (cols.max(1), rows.max(1))
}

fn terminal_command() -> String {
    if let Ok(custom) = std::env::var("TUI_GPU_TERMINAL_COMMAND") {
        return custom;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    format!("printf \"tui-gpu ready\\n\"; exec {}", shell)
}

fn build_terminal_vertices(
    snapshot: &TerminalBufferSnapshot,
    pane_rect: Rect,
    window_width: f32,
    window_height: f32,
    atlas: &text::GlyphAtlas,
) -> Vec<GlyphVertex> {
    if pane_rect.width <= 0.0 || pane_rect.height <= 0.0 {
        return Vec::new();
    }
    let mut vertices = Vec::with_capacity(snapshot.rows * snapshot.cols * 6);
    let pane_right = pane_rect.x + pane_rect.width;
    let pane_bottom = pane_rect.y + pane_rect.height;
    for row in 0..snapshot.rows {
        let y = pane_rect.y + row as f32 * CELL_PIXEL_HEIGHT;
        if y >= pane_bottom {
            break;
        }
        let max_y = y + CELL_PIXEL_HEIGHT;
        if max_y > pane_bottom {
            break;
        }
        for col in 0..snapshot.cols {
            let x = pane_rect.x + col as f32 * CELL_PIXEL_WIDTH;
            if x >= pane_right {
                break;
            }
            let max_x = x + CELL_PIXEL_WIDTH;
            if max_x > pane_right {
                break;
            }
            if let Some(cell) = snapshot.cell(row, col) {
                if cell.ch == ' ' {
                    continue;
                }
                let rect = Rect {
                    x,
                    y,
                    width: CELL_PIXEL_WIDTH,
                    height: CELL_PIXEL_HEIGHT,
                };
                push_glyph_quad(
                    &mut vertices,
                    rect,
                    atlas.glyph_uv(cell.ch),
                    rgb_to_vec3(cell.fg),
                    window_width,
                    window_height,
                );
            }
        }
    }
    vertices
}

fn push_rect(vertices: &mut Vec<Vertex>, rect: Rect, color: [f32; 3], width: f32, height: f32) {
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.width;
    let y1 = rect.y + rect.height;
    let v0 = Vertex {
        position: px_to_ndc(x0, y0, width, height),
        color,
    };
    let v1 = Vertex {
        position: px_to_ndc(x1, y0, width, height),
        color,
    };
    let v2 = Vertex {
        position: px_to_ndc(x1, y1, width, height),
        color,
    };
    let v3 = Vertex {
        position: px_to_ndc(x0, y1, width, height),
        color,
    };
    vertices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
}

fn px_to_ndc(px: f32, py: f32, width: f32, height: f32) -> [f32; 2] {
    let x = (px / width) * 2.0 - 1.0;
    let y = 1.0 - (py / height) * 2.0;
    [x, y]
}

fn special_key_sequence(event: &KeyEvent) -> Option<Vec<u8>> {
    use winit::keyboard::{KeyCode, PhysicalKey};
    if let PhysicalKey::Code(code) = &event.physical_key {
        match code {
            KeyCode::Backspace => Some(vec![0x7f]),
            KeyCode::Enter => Some(b"\r".to_vec()),
            KeyCode::Tab => Some(vec![b'\t']),
            KeyCode::Escape => Some(vec![0x1b]),
            KeyCode::ArrowUp => Some(b"\x1b[A".to_vec()),
            KeyCode::ArrowDown => Some(b"\x1b[B".to_vec()),
            KeyCode::ArrowRight => Some(b"\x1b[C".to_vec()),
            KeyCode::ArrowLeft => Some(b"\x1b[D".to_vec()),
            KeyCode::Home => Some(b"\x1b[H".to_vec()),
            KeyCode::End => Some(b"\x1b[F".to_vec()),
            KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
            KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
            KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
            KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
            _ => None,
        }
    } else {
        None
    }
}

fn load_layout() -> Result<LayoutDocument> {
    let content = std::fs::read_to_string(LAYOUT_PATH)
        .with_context(|| format!("failed to read layout {LAYOUT_PATH}"))?;
    let doc: LayoutDocument = serde_json::from_str(&content)?;
    Ok(doc)
}

struct LayoutEngine {
    taffy: Taffy,
    root: Node,
    nodes: Vec<NodeInfo>,
}

struct NodeInfo {
    component: ComponentKind,
    node: Node,
}

impl LayoutEngine {
    fn new(doc: &LayoutDocument) -> Result<Self> {
        let mut taffy = Taffy::new();
        let mut nodes = Vec::new();
        for node in &doc.nodes {
            let style = style_from_layout_node(node);
            let handle = taffy.new_leaf(style)?;
            nodes.push(NodeInfo {
                component: node.component,
                node: handle,
            });
        }
        let children: Vec<Node> = nodes.iter().map(|n| n.node).collect();
        let root_style = Style {
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Percent(1.0),
            },
            flex_direction: TaffyFlexDirection::Column,
            ..Default::default()
        };
        let root = taffy.new_with_children(root_style, &children)?;
        Ok(Self { taffy, root, nodes })
    }

    fn compute_layout(&mut self, width: f32, height: f32) -> Result<Vec<(ComponentKind, Rect)>> {
        self.taffy.compute_layout(
            self.root,
            Size {
                width: AvailableSpace::Definite(width),
                height: AvailableSpace::Definite(height),
            },
        )?;
        let mut rects = Vec::new();
        for info in &self.nodes {
            let layout = self.taffy.layout(info.node)?;
            rects.push((
                info.component,
                Rect {
                    x: layout.location.x,
                    y: layout.location.y,
                    width: layout.size.width,
                    height: layout.size.height,
                },
            ));
        }
        Ok(rects)
    }
}

fn style_from_layout_node(node: &LayoutNode) -> Style {
    match node.component {
        ComponentKind::MenuBar => Style {
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Points(MENU_BAR_HEIGHT),
            },
            ..Default::default()
        },
        ComponentKind::OverlayRegion => Style {
            position: Position::Absolute,
            inset: TaffyRect {
                left: LengthPercentageAuto::Auto,
                right: LengthPercentageAuto::Auto,
                top: LengthPercentageAuto::Auto,
                bottom: LengthPercentageAuto::Points(0.0),
            },
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Points(OVERLAY_HEIGHT),
            },
            ..Default::default()
        },
        ComponentKind::TerminalPane => Style {
            flex_grow: node.layout.flex_grow,
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Auto,
            },
            ..Default::default()
        },
        ComponentKind::Unknown => Style::DEFAULT,
    }
}

fn push_glyph_quad(
    vertices: &mut Vec<GlyphVertex>,
    rect: Rect,
    uv: text::GlyphUv,
    color: [f32; 3],
    width: f32,
    height: f32,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.width;
    let y1 = rect.y + rect.height;
    let v0 = GlyphVertex {
        position: px_to_ndc(x0, y0, width, height),
        uv: [uv.u0, uv.v0],
        color,
    };
    let v1 = GlyphVertex {
        position: px_to_ndc(x1, y0, width, height),
        uv: [uv.u1, uv.v0],
        color,
    };
    let v2 = GlyphVertex {
        position: px_to_ndc(x1, y1, width, height),
        uv: [uv.u1, uv.v1],
        color,
    };
    let v3 = GlyphVertex {
        position: px_to_ndc(x0, y1, width, height),
        uv: [uv.u0, uv.v1],
        color,
    };
    vertices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
}

fn rgb_to_vec3(color: TermRgb) -> [f32; 3] {
    [
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
    ]
}
