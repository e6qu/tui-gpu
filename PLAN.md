## PLAN
1. Extend `terminal-session` to integrate libvterm:
   - Feed PTY output into libvterm, maintain cell buffer snapshots, expose resize/input APIs.
   - Add tests asserting libvterm captures simple text.
2. Integrate terminal buffer with renderer (glyph cache/text rendering inside pane rects).
3. Enhance layout compiler (nested nodes/z-order) feeding Taffy tree directly.
4. Build runtime event bus/API using runtime-core (REST/WebSocket transports).
5. Add CI + visual smoke tests (tmux/screenshots) for regression coverage.
