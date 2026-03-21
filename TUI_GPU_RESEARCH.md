## GPU-Accelerated TUI Research

### Goals
- Deliver coding-agent friendly TUIs that look textual but render with GPU-backed canvases for latency-sensitive workloads.
- Target macOS (Metal/OpenGL via MoltenVK) and Linux (Vulkan/OpenGL) without depending on heavyweight desktop toolkits.
- Balance predictability of text-grid UX with richer graphics (sub-cell widgets, plots, hints) when terminals expose pixel protocols.

### Constraints and cross-cutting concerns
- **Rendering**: need fast glyph atlas uploads, instanced quads, and partial updates to avoid redrawing full screens each frame.
- **Input + focus**: in headless/remote cases (SSH) we still require compatibility with terminal escape sequences, but rich UIs may instead run in their own GPU windows via `winit`/`SDL`.
- **Text shaping**: HarfBuzz or `swash`/`rustybuzz` for ligatures, emoji, and fallback fonts; pack glyph textures to avoid layout drift between OSes.
- **Testing**: golden-frame comparison plus benchmarks for redraw latency (target <5 ms patch-to-present when glyph cache is warm).

### Candidate approaches

#### 1. `wgpu` text-grid renderer
- Cross-platform Rust API that maps to Vulkan/Metal/DX12/OpenGL automatically.
- Pair with `winit` for event loop, `glyphon` or `swash` for GPU glyph cache, and a TUI layout engine such as `ratatui`/`tui-rs` for box layouts.
- Pros: single codebase for macOS + Linux; modern API with bind groups, compute shaders for damage tracking, integrates easily with Rust agent code.
- Challenges: need to author custom text renderer (or integrate `glyphon`), manage swap-chain resizing, falls back to software on older Macs without Metal via OpenGL backend.

#### 2. Raw OpenGL/GLFW backend
- Use `GLFW` or `SDL3` to open a window, drive OpenGL 3.3+ (Mac supports up to 4.1 core) and render glyph quads via instanced draws.
- Mature ecosystem (NanoVG, `freetype-gl`, `gltext`) and easy prototyping in C/C++ or Rust (`glutin`).
- Pros: simpler shader model, abundant tutorials, low start-up cost, can interop with ImGui for debugging overlays.
- Challenges: Apple freezes OpenGL at 4.1, so advanced compute tricks unavailable; driver variability on Linux; need extra work for HDR / high DPI.

#### 3. Vulkan / `vulkano` / `ash` pipeline
- Gives explicit control over memory, synchronization, and subpass composition, helpful for streaming >10k glyph updates per frame.
- Enables compute shader driven cell diffing and direct storage-buffer writes from async workers.
- Pros: predictable performance, zero-copy staging, ability to share device with other GPU workloads (LLM token visualizers, volumetric plots).
- Challenges: steep learning curve, MoltenVK translation layer on macOS increases maintenance cost, validation layers complicate CI.

#### 4. GPU-accelerated terminal protocols (Kitty, WezTerm, iTerm2)
- Run inside an existing GPU terminal but exploit its pixel protocols: Kitty graphics, iTerm2 inline images, Sixel, or the planned `ttyd` protocol.
- Application stays a standard terminal binary, yet can blit RGBA tiles for graphs, previews, or agent visual context.
- Pros: preserves SSH workflow, no extra window manager integration, backwards-compatible (falls back to ASCII art).
- Challenges: capability detection per terminal, throughput limits (Kitty caps at ~1GB/s), need compression to avoid swamping pty.

#### 5. Skia/NanoVG canvas with textual styling
- Use Skia (via `skia-safe` Rust bindings or C++) or NanoVG for GPU tessellation plus text shaping.
- Build a retained tree representing TUI panels; render via Skia's GPU backend (Metal on macOS, Vulkan/GL on Linux).
- Pros: built-in paragraph shaping, subpixel positioning, vector icons; good when TUIs want more than monospace grids.
- Challenges: heavier dependency, Skia build tooling complex, may feel less “terminal-like”.

### Supporting building blocks
- **Input layers**: `winit`, `SDL`, or `glfw` provide cross-platform keyboard + IME support (needed for coding agents handling multi-lingual input).
- **Terminal compatibility**: embed `libvterm`/`notcurses` to translate ANSI sequences into our GPU grid when running traditional CLI programs inside the TUI shell.
- **Scripting/agents**: Rust async runtimes or Zig event loops can integrate GPU rendering with agent brains; consider using shared-memory command buffers so model tokens can stream into renderers without blocking.

### Recommendations
1. Prototype a Rust `wgpu` renderer since it offers the cleanest abstraction over Metal/Vulkan and integrates with popular agent runtimes. Focus on: glyph atlas via `glyphon`, compute shader diffing, and bridging with `ratatui` layout data.
2. In parallel, spike a Kitty protocol renderer to evaluate delivering GPU content over SSH without a custom window.
3. Keep Vulkan/`ash` as the long-term “max control” path if we hit `wgpu` limits (e.g., multi-GB/s updates or multi-window compositing).

### Next steps
- Flesh out architecture docs for the `wgpu` path: event loop, render passes, glyph cache invalidation, text cursor pipeline, IME bridging.
- Build microbenchmarks on macOS + Linux GPUs (Apple M-series, AMD integrated, NVIDIA) to measure glyph upload throughput and frame latency.
- Explore packaging: decide between stand-alone binary (winit) vs. plugin inside GPU terminal (Kitty/WezTerm).
