## DO NEXT
- Integrate VT buffer into renderer panes (glyph cache/text rendering pipeline).
- Enhance layout compiler (nested nodes/z-order) feeding Taffy tree directly.
- Build runtime API/event bus atop runtime-core (with tests).
- Add CI + visual smoke tests (tmux/screenshots).
- Later, build a GPU-accelerated spinning-cube/ray tracer demo rendered inside the TUI (serves as a stress/test harness).
- Add native RGB frame ingestion so Doom/bitmap producers can stream pixels without third-party renderers:
  1. ✅ Define renderer input channel + shared frame-feed file (see `frame-feed` crate, `render-app` channel, and `tui-demos feed-*` writers).
  2. Wrap `doomgeneric` (or similar) so `DG_DrawFrame` writes RGB into the channel while PTY input feeds Doom’s event queue (now also mirrored via the input feed).
  3. Extend `render-app` to toggle panes between VT glyphs and RGB textures, routing keyboard/mouse focus accordingly (RGB frames already preempt VT; F9 toggles keyboard routing, but we still need pointer focus + Doom handshake).
  4. Write headless tests that feed recorded frame sequences and verify GPU output (readback/checksum).
  5. Document the workflow + add a `tui-demos doom` example to launch the wrapper and stream frames inside the GPU TUI.
