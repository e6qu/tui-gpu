## GPU TUI Building Blocks

This repository already contains the primitives we need to grow a full GPU-rendered TUI stack. The table below outlines each layer, the code that implements it today, and the way forward if we want to turn it into a reusable library (including room for GPGPU workloads).

| Layer | What it does today | Library building block |
|-------|--------------------|------------------------|
| Layout + overlays | `renderer-core::layout` + `LayoutEngine` drive Taffy nodes, menu bars, and the overlay region. | Extract into a standalone `tui_layout` crate so non-renderer-core consumers can load serialized templates or generate nodes dynamically. |
| Text/glyph system | `renderer-core::text` builds the glyph atlas/pipeline used by GUI + ANSI block rendering. | Promote to a `GpuTextCache` API exposing `push_text(rect, string, color)` and alternate shaders (blocks, braille, MSDF). |
| VT buffer | `terminal-session` maintains a PTY-backed buffer (via `vte`). | Continue exposing `TerminalBufferSnapshot`; long-term we can offer a `TerminalPane` trait that renders either glyph instances (GUI) or ANSI blocks (TUI). |
| Frame feed | `frame-feed` handles RGB + audio + input shared-memory queues. | Keep as-is; Doom, YouTube, and custom producers already reuse the API. |
| Renderer state | `renderer-core` owns wgpu device state, RGB uploaders, PTY input, and the demo multiplexer (`run_app`). | Split into composable structs (`RendererCore`, `GuiBackend`, `TuiBackend`) so other binaries (or tests) can embed a subset (e.g., GPU-only textures without PTY). |
| TUI/GUI shells | `renderer-core::run_gui` and `run_tui` configure feeds, launch winit or ANSI mode, and pick the CPU/GPU compute backend per demo. | Package the shell logic (input routing, raw-mode guard) so consumers like `examples/tui-demos` can drive the same cores without duplicating boilerplate. |

### Toward GPGPU-powered panes

Once the core primitives are reusable, adding a compute-heavy pane is straightforward:

1. Implement a `GpuPane` trait: `fn prepare(&mut self, device, queue, target_format)`, `fn render(&mut self, encoder, target_view, rect)`. The existing RGB frame pane and text pane already behave like this.
2. For compute workloads (raymarching, particle sims), spin up a `wgpu::ComputePipeline` inside the pane; write results into a texture and blit just like `FrameRenderer`.
3. Expose a stable `RenderGraph` so multiple panes (VT, overlays, compute widgets) can share intermediate textures or uniform buffers without duplicating uploads.

### CLI + mode cleanup

`render-app` is now a thin CLI wrapper over `renderer-core::run_app`. It exposes `--mode gui|tui` and `--compute cpu|gpu`, so every demo can mix compute + presentation backends (plasma/ray support both CPU/GPU; Doom/YouTube fall back to CPU). Next steps:

1. Export a `RendererCli` helper (or embed the clap definitions in `renderer-core`) so other binaries can reuse the same flags.
2. Port `examples/tui-demos` / future agent shells to instantiate `renderer-core` directly instead of setting up PTYs and raw mode by hand.
3. Extend the demo set (e.g., live Doom streams with audio/input feeds), proving that external producers can pipe data into both GUI and TUI.

### Immediate vs buffered rendering

To support both immediate-mode rendering (useful for ANSI/PTTY sinks) and double/triple-buffered GPU pipelines:

* **Immediate mode**: keep the current CPU-based ANSI path. Each frame is generated, converted to glyphs, and immediately flushed to the PTY—no swapchain.
* **Double/triple buffering (GUI)**: wgpu already handles swapchain presentation in `run_window_mode`; we can expose a CLI flag in the future to choose between single buffering (for latency) and triple buffering (for throughput) by selecting `present_mode`.
* **Indirect rendering**: GPU demos currently downsample their textures for ANSI output; a future `GpuPane` should publish handles so a compositing pass (or external consumer) can reuse the same GPU work without readbacks.

The long-term goal is already partially realized: `renderer-core` runs the same compute pipeline regardless of GUI/TUI mode, and the block-based ANSI renderer mirrors the GPU window. Next we can expose the render graph so third parties can embed their own panes (metrics, GPGPU sims) alongside the PTY pane.
