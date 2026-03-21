## Project: Kitty (a.k.a. KiTTY) GPU Terminal

### Stack and architecture
- Pure OpenGL 3.x/4.x renderer implemented in C (`kitty/gl.c`, `kitty/shaders.c`); window/input handled through vendored GLFW.
- Glyph atlas + sprite map management maintained in `kitty/shaders.c`, with DMA-like buffer uploads through helper functions in `kitty/gl.c`.
- Python orchestration layer drives layouts/kitten scripting, while hot paths remain native to keep latency low.

### How Kitty talks to the GPU
- Startup builds VAOs/VBOs per renderable layer and caches them globally. Helpers in `kitty/gl.c` wrap GLAD calls to keep contexts consistent even across headless windows.
- Instanced drawing (`glDrawArraysInstanced`) renders a quad per character cell while blending is toggled dynamically (pre-multiplied alpha).
- Glyph and decoration data are streamed via persistent mapped buffers; dirty portions of the atlas are updated with `glTexSubImage2D`.
- Inline image protocol (OSC 1337) reuses GPU textures by referencing server-side IDs, letting TUI apps blit bitmaps without leaving the GL pipeline.

### Sample code (Kitty glyph quad draw)
```c
// kitty/gl.c:110-137
void
draw_quad(bool blend, unsigned instance_count) {
    set_blending(blend);
    if (instance_count)
        glDrawArraysInstanced(GL_TRIANGLE_FAN, 0, 4, instance_count);
    else
        glDrawArrays(GL_TRIANGLE_FAN, 0, 4);
}
```
This helper is invoked for each batch of glyph instances; Kitty precalculates per-instance vertex data, maps it into VBOs via `map_vao_buffer_for_write_only`, and emits a single instanced draw per damage region.

### Notes for agent-style TUIs
- GPU glyph cache is exposed via the Kitty graphics protocol, so an agent can upload entire panes (plots, previews) as textures while the text grid continues to use instanced quads.
- Frame pacing is driven by Kitty's render loop, so applications targeting deterministic latency should minimize full-screen damage and rely on sub-rectangle updates (Kitty keeps dirty rectangles in `window.c`).
- Back-end is tied to OpenGL; macOS relies on the system's compatibility profile, so leveraging compute-like work requires multipass fragment shaders rather than compute stages.
