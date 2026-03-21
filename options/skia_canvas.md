## Option: Skia / NanoVG GPU canvas with TUI styling

**Intent**: Treat the UI as a retained-mode canvas (Skia or NanoVG) but enforce TUI-like visual language (mono fonts, panes, text cursors). Lets us mix crisp typography with charts/previews while reusing a production-grade 2D engine.

### Stack
- Skia (`skia-safe` for Rust or native C++) or NanoVG for GPU-backed drawing.
- Window/event management: `winit`, SDL, or Qt shell (without using Qt widgets).
- Text shaping: Skia Paragraph, HarfBuzz; fallback fonts handled by Skia's font manager.
- Layout: custom flex/grid or reuse `stretch`/`taffy` for deterministic sizing.

### Strengths
- Excellent glyph rendering, subpixel positioning, bidirectional text, fallback fonts.
- Built-in GPU backends: Metal (macOS), OpenGL/Vulkan (Linux), automatically handles surfaces, MSAA, HiDPI.
- Easy to draw non-textual widgets (sparklines, vector icons) without switching contexts.

### Risks / open questions
- Skia build tooling (GN/Ninja) is heavy; adds large dependency surface and longer CI times.
- Harder to guarantee strict monospace layout because Skia encourages pixel-perfect positions; must add grid snapping.
- Less “terminal-like” by default, so need design guidelines to keep UX consistent.

### Prototype focus
1. Build static layout + text style guide to ensure grid snap and keyline alignment.
2. Integrate Skia Paragraph for multi-language editing, verify IME + ligatures.
3. Benchmark redraw of 1,000 widgets (panes, charts) to ensure <10 ms composite time on M2 + AMD iGPU.
