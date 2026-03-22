## PLAN
### Phase 1 — Rebuild the renderer core
1. ✅ Recreate `render-app`’s wgpu surface/device setup, layout loading, and event loop.
2. ✅ Reattach the PTY session + VT buffer so we can render glyphs again (GUI mode now shows the live terminal pane).

### Phase 2 — Bring back demos/feeds
3. Restore Doom frame/input/audio feeds and the RGB texture pipeline.
4. Re-introduce the plasma demo and add the GPU compute raytracer alongside the CPU fallback.

### Phase 3 — Instrumentation + docs
5. Layer FPS counters + CLI switches to compare CPU vs GPU in both GUI and TUI.
6. Refresh layout/runtime docs + tests once the renderer is functional again.
