## Option: OpenGL + GLFW/SDL backend

**Intent**: Use a lean OpenGL 3.3+ renderer for glyph quads with GLFW (or SDL3) handling windows/input. Provides deterministic control while keeping implementation approachable for C/C++ or Rust.

### Stack
- GLFW (or SDL) for creating contexts, handling keyboard/IME, and vsync.
- OpenGL 3.3 core profile shaders for instanced rendering of glyph quads; optional use of persistent mapped buffers.
- `freetype` / `harfbuzz` / `msdfgen` for glyph rasterization (signed-distance fields for crisp scaling).
- Optional: `NanoVG` or Dear ImGui overlays for debugging/perf graphs.

### Strengths
- Very mature tooling (RenderDoc, apitrace, GL profilers) and tons of reference code.
- Works everywhere (macOS ships GL 4.1 core, Linux distros have Mesa/NVIDIA drivers).
- Easier onboarding for contributors familiar with GL vs. Vulkan/Metal.
- IME/text input story already solved by GLFW/SDL.

### Risks / open questions
- Limited future on macOS (Apple freezes GL features; no timeline semaphores/compute); performance ceilings on large glyph counts.
- Need manual fallbacks for retina scaling, HDR, color profiles.
- Must manage GL loader differences (`glad`, `glew`) if supporting multiple languages.

### Prototype focus
1. Build glyph atlas using signed-distance fields to minimize re-rasterization.
2. Implement partial updates with persistent mapped buffer per row to keep latency low.
3. Validate vsync OFF mode to achieve <5 ms input-to-display on both macOS/Linux GPUs.
