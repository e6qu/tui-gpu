## Project: Zed (GPU-Accelerated Code Editor)

### Stack and architecture
- Zed’s GPUI runtime renders all widgets (text editor, panels, terminal) with a custom scene graph that targets multiple backends: `gpui_wgpu` (Metal/Vulkan via wgpu), `gpui_macos` (Metal), and `gpui_windows` (DirectX).
- Scene building happens in Rust, while Swift handles macOS integration (window chrome, shortcuts). Rendering is retained—only changed primitives are re-sent to the GPU.
- Text shaping uses Cosmic Text + custom atlases; each glyph batch becomes a primitive instance for the GPU renderer.

### How Zed interfaces with the GPU
- `crates/gpui_wgpu/src/wgpu_renderer.rs` creates wgpu pipelines, bind groups, and atlases. Render passes iterate over primitive batches (quads, shadows, sprites) and call helper methods like `draw_quads`.
- Instance data for primitives is written into a large GPU buffer via `write_to_instance_buffer`, then bound as storage/uniform buffers when drawing.
- Device loss is handled by recreating `WgpuContext` and reinitializing pipelines/atlases, ensuring resilience on flaky drivers.

### Sample code (Zed render pass)
```rust
// crates/gpui_wgpu/src/wgpu_renderer.rs:1149-1185
let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    label: Some("main_pass"),
    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: &frame_view,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
        depth_slice: None,
    })],
    ..Default::default()
});
for batch in scene.batches() {
    match batch {
        PrimitiveBatch::Quads(range) => {
            self.draw_quads(&scene.quads[range], &mut instance_offset, &mut pass)
        }
        PrimitiveBatch::Paths(range) => { ... }
        PrimitiveBatch::Shadows(range) => { ... }
    }
}
```
`draw_quads` ultimately binds the quad pipeline, sets atlas texture bind groups, and issues `pass.draw(0..4, 0..instance_count)`—mirroring what a GPU TUI might do for glyph quads.

### Notes for agent-friendly TUIs
- GPUI demonstrates how to mix text, vector shapes, and custom widgets (AI chat, diagnostics) in a single GPU scene graph; coding agents can emulate this structure.
- Its robust device-loss handling and atlas management provide blueprints for long-lived agent sessions that must survive GPU restarts.
- Collaborative editing overlays (multi-cursor) show how to render real-time annotations efficiently, a requirement for agent copilots.
