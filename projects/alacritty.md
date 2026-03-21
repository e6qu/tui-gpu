## Project: Alacritty (OpenGL Terminal)

### Stack and architecture
- Rust + OpenGL core profile renderer using `glutin` context creation (see `alacritty/src/renderer/mod.rs`).
- Text pipeline implemented twice: GLSL3 path for OpenGL 3.3+ and GLES2 fallback for mobile/older GPUs (`renderer/text/glsl3.rs`, `renderer/text/gles2.rs`).
- Glyph rasterization handled by `crossfont`; glyphs packed into atlases that live as OpenGL textures and are updated with `glTexSubImage2D`.

### GPU interface details
- Renderer selects GLSL3 vs GLES2 at runtime (`Renderer::new`) depending on reported GL version and config.
- Each render frame builds batches of glyph instances and rectangles, then issues instanced draws; color data and vertex positions are interleaved to minimize buffer binds.
- Uniforms (projection, cursor info) are pushed per shader pass; background, subpixel, and foreground passes reuse the same quad indices.
- Scrollback + damage: rectangles for underline/selection/scroll regions are drawn by `RectRenderer`, using streaming VBO uploads (`glBufferData(GL_STREAM_DRAW)`).

### Sample code (Alacritty instanced glyph draw)
```rust
// alacritty/src/renderer/text/glsl3.rs:233-256
self.program.set_rendering_pass(RenderingPass::Background);
gl::DrawElementsInstanced(
    gl::TRIANGLES,
    6,
    gl::UNSIGNED_INT,
    ptr::null(),
    self.batch.len() as GLsizei,
);
self.program.set_rendering_pass(RenderingPass::SubpixelPass1);
gl::DrawElementsInstanced(
    gl::TRIANGLES,
    6,
    gl::UNSIGNED_INT,
    ptr::null(),
    self.batch.len() as GLsizei,
);
```
`RenderApi` streams glyph vertices into a dynamic buffer; when the batch fills, it issues the draws above to paint background and subpixel passes with the active glyph texture bound.

### Notes for agent workloads
- Minimalism (no scripting) means agent integrations must run separate control planes (tmux, OSC 52, etc.) but benefit from predictable rendering.
- Because glyph caching and batching are straightforward, Alacritty is a good reference when designing GPU TUIs that require explicit control over instanced draws.
- To add inline graphics, one would extend the GLSL pipeline (e.g., additional texture units) mirroring what agent prototypes might need.
