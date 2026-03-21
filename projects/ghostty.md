## Project: Ghostty (GPU Terminal for macOS/Linux)

### Stack and architecture
- Zig-based renderer with pluggable backends (`src/renderer/OpenGL.zig`, `Metal.zig`, `WebGL.zig`); production path today is OpenGL 4.3+ via GLFW.
- Rendering is abstracted into `Renderer = GenericRenderer(OpenGL)` – the GenericRenderer produces cell batches, while the backend handles actual draw calls.
- Build system emits shaders for both OpenGL and Metal, ensuring matching visual output across OSes.

### GPU interface details
- OpenGL backend wraps GLAD and demands >=4.3 for instanced rendering + SSBO usage. Context preparation (`prepareContext`) enables debug callbacks and SRGB framebuffers.
- Render passes described in `src/renderer/opengl/RenderPass.zig`; each `Step` packages pipeline, uniform buffer, textures, and draw counts, giving explicit control over binding order.
- Multi-buffering handled manually: vertex/instance buffers bound to slot 0, additional SSBOs for per-cell metadata bound via `glBindBufferBase`.

### Sample code (Ghostty render pass draw)
```zig
// src/renderer/opengl/RenderPass.zig:70-118
pub fn step(self: *Self, s: Step) void {
    if (s.draw.instance_count == 0) return;
    const pbind = s.pipeline.program.use() catch return;
    const vaobind = s.pipeline.vao.bind() catch return;
    const fbobind = ...;
    if (self.step_number == 0) if (self.attachments[0].clear_color) |c| {
        gl.clearColor(c[0], c[1], c[2], c[3]);
        gl.clear(gl.c.GL_COLOR_BUFFER_BIT);
    };
    if (s.uniforms) |ubo| {
        _ = ubo.bindBase(.uniform, 1) catch return;
    }
    ...
    gl.drawArraysInstanced(
        s.draw.type,
        0,
        @intCast(s.draw.vertex_count),
        @intCast(s.draw.instance_count),
    ) catch return;
}
```
The Step API centralizes OpenGL state; higher-level renderer code simply defines Step structs per batch of glyphs/decals, letting Ghostty mix pipelines (text, rectangles, shader overlays) per frame.

### Notes for agent workflows
- Because Zig exposes renderer internals, it is feasible to add RPC hooks for injecting additional render steps (e.g., overlaying agent hints) without rewriting the render loop.
- Multi-backend design provides a path to Metal (macOS) without rewriting application logic; agents targeting Apple hardware can expect consistent GPU semantics once the Metal backend stabilizes.
- Rendering stack already supports tab/split management, so agent TUIs can rely on Ghostty for workspace layout while focusing on GPU-friendly content.
