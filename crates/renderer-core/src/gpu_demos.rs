use anyhow::{bail, Context, Result};
use futures_intrusive::channel::shared::oneshot_channel;

use crate::{rgb_frame::RgbFrame, DemoKind, RGB_DEMO_HEIGHT, RGB_DEMO_WIDTH};

const WORKGROUP_SIZE: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    resolution: [f32; 2],
    time: f32,
    _pad: f32,
}

pub trait GpuDemo {
    fn update(&mut self, dt: f32, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()>;
    fn texture_view(&self) -> &wgpu::TextureView;
    fn capture_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<RgbFrame>;
}

pub fn create_gpu_demo(
    demo: DemoKind,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Box<dyn GpuDemo>> {
    match demo {
        DemoKind::Plasma => Ok(Box::new(GenericGpuDemo::new(
            device,
            queue,
            include_str!("plasma.wgsl"),
            "plasma-gpu-demo",
        )?)),
        DemoKind::Ray => Ok(Box::new(GenericGpuDemo::new(
            device,
            queue,
            include_str!("raytracer.wgsl"),
            "ray-gpu-demo",
        )?)),
        _ => bail!("GPU mode is not available for demo {:?}", demo),
    }
}

struct GenericGpuDemo {
    width: u32,
    height: u32,
    time: f32,
    params_buffer: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    staging: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

impl GenericGpuDemo {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_src: &str,
        label: &str,
    ) -> Result<Self> {
        let width = RGB_DEMO_WIDTH;
        let height = RGB_DEMO_HEIGHT;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label}-params")),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("{label}-texture")),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("{label}-shader")),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("{label}-bind-layout")),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{label}-pipeline-layout")),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(&format!("{label}-pipeline")),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{label}-bind-group")),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });
        let bytes_per_row = width * 4;
        let padded_bytes_per_row = align_to(
            bytes_per_row as usize,
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize,
        ) as u32;
        let staging_size = padded_bytes_per_row as u64 * height as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label}-staging")),
            size: staging_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let demo = Self {
            width,
            height,
            time: 0.0,
            params_buffer,
            pipeline,
            bind_group,
            texture,
            view,
            staging,
            padded_bytes_per_row,
        };
        demo.write_params(queue);
        Ok(demo)
    }

    fn write_params(&self, queue: &wgpu::Queue) {
        let params = Params {
            resolution: [self.width as f32, self.height as f32],
            time: self.time,
            _pad: 0.0,
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));
    }
}

impl GpuDemo for GenericGpuDemo {
    fn update(&mut self, dt: f32, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
        self.time += dt;
        self.write_params(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gpu-demo-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            let x = (self.width + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            let y = (self.height + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            pass.dispatch_workgroups(x, y, 1);
        }
        queue.submit(Some(encoder.finish()));
        Ok(())
    }

    fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    fn capture_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<RgbFrame> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gpu-demo-readback"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &self.staging,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        let slice = self.staging.slice(..);
        let (sender, receiver) = oneshot_channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            sender.send(res).ok();
        });
        device.poll(wgpu::Maintain::Wait);
        pollster::block_on(receiver.receive())
            .context("map_async for GPU demo frame failed")?
            .context("map_async callback dropped")?;
        let data = slice.get_mapped_range();
        let mut pixels = vec![0u8; (self.width * self.height * 3) as usize];
        let row_bytes = (self.width * 4) as usize;
        for row in 0..self.height as usize {
            let src_offset = row * self.padded_bytes_per_row as usize;
            let src = &data[src_offset..src_offset + row_bytes];
            let dst_offset = row * self.width as usize * 3;
            for (chunk, out) in src
                .chunks_exact(4)
                .zip(pixels[dst_offset..dst_offset + (self.width as usize * 3)].chunks_exact_mut(3))
            {
                out[0] = chunk[0];
                out[1] = chunk[1];
                out[2] = chunk[2];
            }
        }
        drop(data);
        self.staging.unmap();
        Ok(RgbFrame::new(self.width, self.height, pixels))
    }
}

fn align_to(value: usize, alignment: usize) -> usize {
    if value % alignment == 0 {
        value
    } else {
        value + alignment - (value % alignment)
    }
}
