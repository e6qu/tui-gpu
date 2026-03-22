## DO NEXT
- Hook libvterm into `terminal-session` to maintain cell buffers and surface resize/input APIs.
- Once buffer API is ready, render glyphs in pane rects (glyph cache/MSDF pipeline).
- Enhance layout compiler (nested nodes/z-order) and tie outputs directly to Taffy tree creation.
- Build runtime API/event bus atop runtime-core.
