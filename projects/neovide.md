## Project: Neovide (GPU Neovim GUI)

### Stack and architecture
- Rust front-end that speaks Neovim RPC and renders via Skia + backend-specific surface (OpenGL, Metal, D3D). `renderer/opengl.rs` demonstrates GL path; `renderer/metal.rs`/`d3d.rs` equivalents exist per OS.
- Uses `skia-safe`'s GPU `DirectContext` to render text grids, cursor animations, and blending effects. `renderer/mod.rs` chooses backend based on platform and config.
- Event loop uses winit; swapchain surfaces created via `glutin`, `metal-rs`, or `wgpu`-backed contexts depending on backend.

### GPU interface details
- `OpenGLSkiaRenderer::new` (excerpt below) creates a GL context, loads function pointers, and wires Skia's `DirectContext` to the default framebuffer, enabling Skia to issue GPU draws directly.
- Each frame, Neovide draws into a Skia surface backed by the GL framebuffer; `flush_and_submit` ensures GPU work completes before `swap_buffers`.
- Animations (cursor trails, transparency) rely on Skia shaders executed on the GPU, reducing CPU load.

### Sample code (Skia GL context bootstrap)
```rust
// src/renderer/opengl.rs:76-134
let context = unsafe { gl_display.create_context(&config, &context_attributes) }
    .expect("Failed to create OpenGL context")
    .make_current(&window_surface)
    .unwrap();
gl::load_with(|s| get_proc_address(&window_surface, CString::new(s).unwrap().as_c_str()));
let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
    if name == "eglGetCurrentDisplay" {
        return std::ptr::null();
    }
    get_proc_address(&window_surface, CString::new(name).unwrap().as_c_str())
}).expect("Could not create interface");
let mut gr_context = skia_safe::gpu::direct_contexts::make_gl(interface, None)
    .expect("Could not create direct context");
```
This bootstraps Skia so that later rendering (`SkiaRenderer::canvas`) can draw Neovim's grid via GPU commands.

### Notes for agent workflows
- Although Neovide is purpose-built for Neovim, its architecture (Skia over GL/Metal) mirrors what agent-friendly GPU TUIs need: a retained scene graph, GPU text shaping, and multi-backend support.
- Cursor/animation effects reside in GLSL/Skia shaders (`renderer/shaders`), so agents could adapt similar shader-based overlays to emphasize suggestions or highlight diagnostics.
- VSync implementations differ per OS (Wayland, macOS DisplayLink, etc.), giving a reference for how to keep GPU TUIs in lockstep with compositor timing.
