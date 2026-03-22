## GPU TUI Building Blocks

This repository already contains the primitives we need to grow a full GPU-rendered TUI stack. The table below outlines each layer, the code that implements it today, and the way forward if we want to turn it into a reusable library (including room for GPGPU workloads).

| Layer | What it does today | Library building block |
|-------|--------------------|------------------------|
| Layout + overlays | `layout.rs` + `LayoutEngine` drive Taffy nodes, menu bars, and the overlay region. | Extract into a `tui_layout` crate that exposes `LayoutDocument` + serialized templates so renderers can load/override nodes at runtime. |
| Text/glyph system | `text.rs` builds a glyph atlas and pipelines. | Wrap into a `GpuTextCache` (atlas + pipeline) that provides `push_text(rect, string, color)` style calls. This is the natural injection point for custom glyph sets and block/braille shaders. |
| VT buffer | `terminal-session` maintains a PTY-backed buffer (via `vte`). | Keep exporting `TerminalBufferSnapshot`; in a library we’d expose a trait `TerminalRenderer` that turns snapshots into GPU vertices. |
| Frame feed | `frame-feed` handles RGB + audio + input shared-memory queues. | Keep as-is; the API is already generic enough for any producer (Doom, video players, custom compositors). |
| Renderer state | `State` holds wgpu device/pipelines, latest RGB frame, PTY, layout. | Split into a `RendererCore` that can render arbitrary panes (`GlyphLayer`, `FrameLayer`, `OverlayLayer`). Consumers would wire their own demo content and still get PTY + RGB for free. |
| TUI/GUI shells | `run_window_mode` and `run_tui_mode` configure feeds, launch winit or ANSI mode. | Wrap the shared plumbing (`FeedPaths`, `AudioPlayer`, `InputRouter`) into reusable structs so other binaries (e.g., a future `tui-desktop`) can embed them. |

### Toward GPGPU-powered panes

Once the core primitives are reusable, adding a compute-heavy pane is straightforward:

1. Implement a `GpuPane` trait: `fn prepare(&mut self, device, queue, target_format)`, `fn render(&mut self, encoder, target_view, rect)`. The existing RGB frame pane and text pane already behave like this.
2. For compute workloads (raymarching, particle sims), spin up a `wgpu::ComputePipeline` inside the pane; write results into a texture and blit just like `FrameRenderer`.
3. Expose a stable `RenderGraph` so multiple panes (VT, overlays, compute widgets) can share intermediate textures or uniform buffers without duplicating uploads.

### CLI + mode cleanup

The `render-app` binary now exposes `--mode gui|tui` (with `--tui` kept as a legacy alias) and the shared `--demo`/`--doom-*` flags behave consistently across both modes. TUI rendering can be throttled with `--tui-sample-ms`, which is useful when driving demos through `tmux` or automated scripts. This is the first step toward turning `render-app` into a demo launcher that exercises the future library. Next steps:

* Publish a `render-core` crate that exposes `RendererCore` + `TuiRunner`.
* Port `examples/tui-demos` to instantiate the shared core instead of spinning their own PTYs.
* Add optional GPGPU demos (raymarched cube, plasma) as `GpuPane` implementations to prove out the API.

### Immediate vs buffered rendering

To support both immediate-mode rendering (useful for ANSI/PTTY sinks) and double/triple-buffered GPU pipelines:

* **Immediate mode**: keep the current CPU-based ANSI path. Each frame is generated, converted to glyphs, and immediately flushed to the PTY—no swapchain.
* **Double/triple buffering (GUI)**: wgpu already handles swapchain presentation in `run_window_mode`; we can expose a CLI flag in the future to choose between single buffering (for latency) and triple buffering (for throughput) by selecting `present_mode`.
* **Indirect rendering**: future `GpuPane` implementations should publish command buffers or texture handles that a compositing pass can consume. That enables headless compute passes (raytracer, physics) feeding both GUI (direct blit) and TUI (downsampled glyphs) without re-running the workload twice.

The long-term goal: keep a `RendererCore` running even in `--mode tui`, use compute shaders to populate textures, then choose the presentation strategy per output (immediate ANSI vs swapchain-backed GUI). This gives us the flexibility to showcase both modern GPGPU techniques and the classic ANSI terminal output from the same code paths.
