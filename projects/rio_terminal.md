## Project: Rio (Rust GPU Terminal)

### Stack and architecture
- Rio wraps a custom renderer named Sugarloaf (`sugarloaf/` crate) that targets `wgpu`. High-level terminal UI lives in `frontends/rioterm`.
- `rio-window` abstracts winit/wgpu integration (surface creation, event loop). `sugarloaf` handles glyph layout, style, and GPU resource lifetime.
- Backends selectable via config (Vulkan, Metal, OpenGL via wgpu backends). Terminal logic streams cell diffs to Sugarloaf each frame.

### How Rio uses the GPU
- `sugarloaf::renderer` builds glyph vertices, writes them into a `wgpu::Buffer`, and draws using a single render pipeline per batch.
- Background color / clear behavior is dynamic: render pass chooses between `LoadOp::Clear` or `LoadOp::Load` depending on requested background.
- Glyph + cursor textures are bound via two bind groups (constants and layout) before issuing `rpass.draw`.
- Additional pipelines for shaders/filters live in `sugarloaf/src/components/filters`, allowing post-processing like CRT shaders using compute workloads.

### Sample code (Sugarloaf render pass)
```rust
// sugarloaf/src/renderer/mod.rs:1794-1810
rpass.set_pipeline(&self.pipeline);
rpass.set_bind_group(0, &self.constant_bind_group, &[]);
rpass.set_bind_group(1, &self.layout_bind_group, &[]);
rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

let vertex_count = vertices.len() as u32;
rpass.draw(0..vertex_count, 0..1);
```
Vertices (position, UVs, colors) are copied into `self.vertex_buffer` earlier in the function, so every damage batch results in exactly one indexed draw.

### Notes for TUIs/agents
- Sugarloaf exposes filter stacks and bind-group management, making it possible to add agent-specific pipelines (e.g., highlight overlays) without touching terminal code.
- Since Rio already depends on `wgpu`, most GPU experimentation (compute shaders for parsing, storage buffers for diffing) can be added with minimal portability work.
- Async architecture (renderer on own task) aligns with coding agents that might stream data into GPU buffers without blocking PTY processing.
