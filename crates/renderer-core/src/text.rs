use font8x8::legacy::BASIC_LEGACY;
use wgpu::util::{DeviceExt, TextureDataOrder};

pub const CELL_PIXEL_WIDTH: f32 = 9.0;
pub const CELL_PIXEL_HEIGHT: f32 = 16.0;

const ATLAS_GLYPH_START: u8 = 32;
const ATLAS_GLYPH_END: u8 = 126;
const ATLAS_GLYPHS_PER_ROW: u32 = 16;
const GLYPH_PIXELS: (u32, u32) = (8, 8);
const GLYPH_MAP_SIZE: usize = 256;
const FALLBACK_CHAR: char = '?';

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlyphVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 3],
}

impl GlyphVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x3];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<GlyphVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GlyphUv {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

pub struct GlyphAtlas {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    glyph_map: [Option<GlyphUv>; GLYPH_MAP_SIZE],
    fallback_uv: GlyphUv,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let glyph_count = (ATLAS_GLYPH_END - ATLAS_GLYPH_START + 1) as u32;
        let rows = (glyph_count + ATLAS_GLYPHS_PER_ROW - 1) / ATLAS_GLYPHS_PER_ROW;
        let atlas_width = ATLAS_GLYPHS_PER_ROW * GLYPH_PIXELS.0;
        let atlas_height = rows * GLYPH_PIXELS.1;
        let mut pixels = vec![0u8; (atlas_width * atlas_height) as usize];

        let mut glyph_map: [Option<GlyphUv>; GLYPH_MAP_SIZE] = [None; GLYPH_MAP_SIZE];
        for (idx, code) in (ATLAS_GLYPH_START..=ATLAS_GLYPH_END).enumerate() {
            let pattern = BASIC_LEGACY.get(code as usize).copied().unwrap_or([0; 8]);
            let atlas_row = (idx as u32) / ATLAS_GLYPHS_PER_ROW;
            let atlas_col = (idx as u32) % ATLAS_GLYPHS_PER_ROW;
            let x_offset = atlas_col * GLYPH_PIXELS.0;
            let y_offset = atlas_row * GLYPH_PIXELS.1;
            for row in 0..GLYPH_PIXELS.1 {
                let bits = pattern[row as usize];
                for col in 0..GLYPH_PIXELS.0 {
                    let mask = 1 << (7 - col);
                    let on = (bits & mask) != 0;
                    let global_x = x_offset + col;
                    let global_y = y_offset + row;
                    let index = (global_y * atlas_width + global_x) as usize;
                    pixels[index] = if on { 0xFF } else { 0x00 };
                }
            }
            let u0 = x_offset as f32 / atlas_width as f32;
            let v0 = y_offset as f32 / atlas_height as f32;
            let u1 = (x_offset + GLYPH_PIXELS.0) as f32 / atlas_width as f32;
            let v1 = (y_offset + GLYPH_PIXELS.1) as f32 / atlas_height as f32;
            glyph_map[code as usize] = Some(GlyphUv { u0, v0, u1, v1 });
        }

        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("glyph-atlas"),
                size: wgpu::Extent3d {
                    width: atlas_width,
                    height: atlas_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            TextureDataOrder::LayerMajor,
            &pixels,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let fallback_uv = glyph_map[FALLBACK_CHAR as usize].unwrap_or(GlyphUv::default());

        Self {
            _texture: texture,
            view,
            sampler,
            glyph_map,
            fallback_uv,
        }
    }

    pub fn glyph_uv(&self, ch: char) -> GlyphUv {
        self.glyph_map
            .get(ch as usize)
            .and_then(|uv| *uv)
            .unwrap_or(self.fallback_uv)
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }
}

pub struct TextPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    atlas: GlyphAtlas,
}

impl TextPipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let atlas = GlyphAtlas::new(device, queue);
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text-bind-group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(atlas.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(atlas.sampler()),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[GlyphVertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            pipeline,
            bind_group,
            atlas,
        }
    }

    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }
}
