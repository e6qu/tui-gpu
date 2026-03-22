## PLAN
1. Wire the new VT buffer into the renderer (glyph cache + text rendering inside pane rectangles).
2. Extend layout compiler (nested nodes/z-order) feeding the Taffy tree directly.
3. Build runtime event bus/API with tests (REST/WebSocket adapters).
4. Add CI + visual smoke tests (tmux/screenshots) for regressions.
