## Renderer Text Integration Plan

1. **Terminal feed**
   - Add an async/background task that drives `terminal-session::TerminalSession` (PTY + `vte`).
   - Expose `TerminalBufferSnapshot` via channel/Arc so the renderer can fetch the latest text grid per frame.
   - Tests: integration test that spawns `printf hello`, waits for snapshot, verifies grid characters.

2. **Glyph cache**
   - Use `fontdue` or `swash` to rasterize glyphs.
   - Store glyph bitmaps in a wgpu texture atlas (MSDF later). Manage atlas updates + staging buffers.
   - Tests: unit test for glyph cache (render `A`, ensure atlas entry created). Snapshot with `wgpu` readback if needed.

3. **Render pipeline**
   - Convert pane rectangles (Taffy output) + terminal grid into instanced quads referencing atlas UVs.
   - Add wgpu render pass for text layer (one pipeline with alpha blending, per-instance color/UV).
   - Tests: GPU smoke test using headless `wgpu` + `TerminalBuffer` fixture → compare pixel output (or at least ensure draw call runs).

4. **Input & resize**
   - Forward keyboard/mouse events from renderer → TerminalSession (write to PTY, adjust `TerminalBuffer`).
   - On pane resize, call `TerminalSession::resize` and rebuild atlas if DPI changes.
   - Tests: spawn `cat`, send key input, verify buffer updates.

5. **Visual verification**
   - Add manual screenshot harness (tmux/script) to capture frames and compare to expected output (UTF-8 text, colors).
   - Later, extend to GPU demo (spinning cube) to stress glyph + layout pipelines.
