use std::sync::Arc;

use anyhow::Result;
mod layout;
use layout::LayoutDocument;
use log::info;
use wgpu::util::DeviceExt;

use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};

const MENU_BAR_HEIGHT: f32 = 48.0;
const OVERLAY_HEIGHT: f32 = 32.0;
const LAYOUT_PATH: &str = "generated/layouts/main_screen.json";

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

struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: LayoutDocument,
}

impl State {
    async fn new(window: Arc<winit::window::Window>) -> Result<Self> {
        let layout = load_layout()?;
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("No suitable GPU adapters found"))?;

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

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            layout,
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

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let window_width = self.config.width.max(1) as f32;
        let window_height = self.config.height.max(1) as f32;
        let vertices = build_layout_vertices(&self.layout, window_width, window_height);
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("layout-vertices"),
                contents: bytemuck::cast_slice(&vertices),
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
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.06,
                            b: 0.09,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.draw(0..vertices.len() as u32, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let event_loop = EventLoop::new()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("tui-gpu prototype")
            .build(&event_loop)?,
    );

    let mut state = pollster::block_on(State::new(window.clone()))?;
    let window_for_loop = window.clone();

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { window_id, event } if window_id == window_for_loop.id() => {
                match event {
                    WindowEvent::CloseRequested => target.exit(),
                    WindowEvent::KeyboardInput { event, .. } => {
                        handle_keyboard_event(&event, target)
                    }
                    WindowEvent::Resized(new_size) => state.resize(new_size),
                    WindowEvent::ScaleFactorChanged { .. } => {
                        state.resize(window_for_loop.inner_size())
                    }
                    WindowEvent::RedrawRequested => match state.render() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => state.resize(window_for_loop.inner_size()),
                        Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                        Err(err) => info!("render error: {err:?}"),
                    },
                    _ => {}
                }
            }
            Event::AboutToWait => window_for_loop.request_redraw(),
            _ => {}
        }
    })?;

    Ok(())
}

fn handle_keyboard_event(event: &KeyEvent, target: &winit::event_loop::EventLoopWindowTarget<()>) {
    if event.state == ElementState::Pressed {
        if let PhysicalKey::Code(KeyCode::Escape) = event.physical_key {
            target.exit();
        }
    }
}

fn load_layout() -> Result<LayoutDocument> {
    let content = std::fs::read_to_string(LAYOUT_PATH)
        .map_err(|e| anyhow::anyhow!("failed to read layout {LAYOUT_PATH}: {e}"))?;
    let doc: LayoutDocument = serde_json::from_str(&content)?;
    Ok(doc)
}

fn build_layout_vertices(doc: &LayoutDocument, width: f32, height: f32) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    let mut top_offset = 0.0;
    let mut bottom_offset = 0.0;

    let mut menu_nodes = Vec::new();
    let mut overlay_nodes = Vec::new();
    let mut pane_nodes = Vec::new();

    for node in &doc.nodes {
        match node.component {
            layout::ComponentKind::MenuBar => menu_nodes.push(node),
            layout::ComponentKind::OverlayRegion => overlay_nodes.push(node),
            layout::ComponentKind::TerminalPane => pane_nodes.push(node),
            layout::ComponentKind::Unknown => {}
        }
    }

    for _node in menu_nodes {
        let rect = Rect {
            x: 0.0,
            y: top_offset,
            width,
            height: MENU_BAR_HEIGHT,
        };
        push_rect(&mut vertices, rect, [0.3, 0.3, 0.35], width, height);
        top_offset += MENU_BAR_HEIGHT;
    }

    for _node in overlay_nodes.iter().rev() {
        let rect = Rect {
            x: 0.0,
            y: height - bottom_offset - OVERLAY_HEIGHT,
            width,
            height: OVERLAY_HEIGHT,
        };
        push_rect(&mut vertices, rect, [0.25, 0.65, 0.25], width, height);
        bottom_offset += OVERLAY_HEIGHT;
    }

    let available = (height - top_offset - bottom_offset).max(0.0);
    if !pane_nodes.is_empty() {
        let total_grow: f32 = pane_nodes.iter().map(|n| n.layout.flex_grow.max(0.0)).sum();
        let fallback_ratio = 1.0 / pane_nodes.len() as f32;
        let mut current_y = top_offset;
        for node in pane_nodes.iter() {
            let ratio = if total_grow > 0.0 {
                node.layout.flex_grow.max(0.0) / total_grow
            } else {
                fallback_ratio
            };
            let rect_height = (available * ratio).max(0.0);
            let rect = Rect {
                x: 0.0,
                y: current_y,
                width,
                height: rect_height,
            };
            push_rect(&mut vertices, rect, [0.15, 0.35, 0.65], width, height);
            current_y += rect_height;
        }
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

fn push_rect(vertices: &mut Vec<Vertex>, rect: Rect, color: [f32; 3], width: f32, height: f32) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let to_ndc = |px: f32, py: f32| -> [f32; 2] {
        let x = (px / width) * 2.0 - 1.0;
        let y = 1.0 - (py / height) * 2.0;
        [x, y]
    };

    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.width;
    let y1 = rect.y + rect.height;

    let v0 = Vertex {
        position: to_ndc(x0, y0),
        color,
    };
    let v1 = Vertex {
        position: to_ndc(x1, y0),
        color,
    };
    let v2 = Vertex {
        position: to_ndc(x1, y1),
        color,
    };
    let v3 = Vertex {
        position: to_ndc(x0, y1),
        color,
    };

    vertices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
}
