## Option: Rust + `wgpu` text-grid renderer

**Intent**: Build a custom renderer that keeps a logical text grid (compatible with TUI paradigms) while pushing pixels via `wgpu` so the same code targets Vulkan (Linux) and Metal (macOS).

### Stack
- `wgpu` for GPU abstraction (maps to Vulkan/Metal/DirectX/OpenGL automatically).
- `winit` for cross-platform window/events; optionally `copypasta` for clipboard.
- `glyphon` / `swash` / `rustybuzz` for glyph rasterization + shaping.
- High-level TUI layout: `ratatui` or bespoke flex/grid engine feeding cell diffs.

### Strengths
- One renderer hits macOS, Linux, and even Windows without ifdefs; zero need to maintain raw graphics backends.
- Compute-friendly API; can add compute shader damage detection to push only changed cells.
- Rust ecosystem meshes with coding-agent runtime (async tasks, LSP servers, etc.).
- Integrates nicely with GPU-accelerated extras (OpenGL interoperability, screen capture for model context).

### Risks / open questions
- Need to invest in glyph cache + atlas streaming; `glyphon` is young compared to Skia.
- Swap-chain management and resizing bugs are common; must handle HDR/high-DPI.
- Requires bundling windowing stack (not a drop-in replacement for running inside legacy terminals).

### Prototype focus
1. Render 200x60 glyph grid, verify <2 ms per frame update with 5% cell changes.
2. Support emoji + ligatures via `swash` fallback fonts.
3. Wire `ratatui` buffer diff to GPU storage buffer and measure throughput.
