## Project: WezTerm (GPU Terminal + Multiplexer)

### Stack and architecture
- Rust application built on `winit` for window/event handling and `wgpu` for GPU abstraction (Metal on macOS, Vulkan/DX12 on others).
- Rendering pipeline defined in `wezterm-gui/src/termwindow/webgpu.rs` & `render/draw.rs`; glyph shaping via HarfBuzz, grid diffing via Rust buffers stored in `TermWindow::render_state`.
- Multiplexer/tabs implemented at the terminal level, but renderers treat each pane as a layer with its own vertex/index buffers.

### How WezTerm interfaces with the GPU
- Instantiates a `wgpu::Device` + swap chain, builds pipeline layouts with uniform + texture bind groups (one for glyph atlas, one for decorations).
- Each frame builds vertex buffers for layers, writes them into `wgpu::Buffer`s, and submits a render pass that binds the glyph atlas & uniform data before drawing indexed quads.
- Damage tracking: vertex buffers are double-buffered per layer (`render_state.layers[vb_idx]`) to allow frame-over-frame reuse.
- Additional passes (e.g., WebGPU front-end for WebAssembly) reuse the same pipeline descriptors, giving parity across platforms.

### Sample code (WezTerm `wgpu` render pass)
```rust
// wezterm-gui/src/termwindow/render/draw.rs:111-138
let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    label: Some("Render Pass"),
    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: &view,
        resolve_target: None,
        ops: wgpu::Operations {
            load: if cleared { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) },
            store: wgpu::StoreOp::Store,
        },
        depth_slice: None,
    })],
    ..Default::default()
});
render_pass.set_pipeline(&webgpu.render_pipeline);
render_pass.set_bind_group(0, &uniforms, &[]);
render_pass.set_bind_group(1, &texture_linear_bind_group, &[]);
render_pass.set_bind_group(2, &texture_nearest_bind_group, &[]);
render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
render_pass.set_index_buffer(vb.indices.webgpu().slice(..), wgpu::IndexFormat::Uint32);
render_pass.draw_indexed(0..index_count as _, 0, 0..1);
```
This pass renders one layer of the terminal; bindings provide projection uniforms plus linear/nearest sampling views of the glyph atlas texture.

### Notes for TUIs/agents
- Lua automation layer can inject overlays rendered as additional layers, so coding agents can paint UI hints without forking the renderer.
- WebGPU backend means the same render path can hypothetically target browsers or remote surfaces if agents need to stream UI.
- Because WezTerm handles glyph caches internally, agent integrations should pass text buffers instead of manual glyph bitmaps unless using its inline image protocol.
