## AGENTS
- Agent sessions have UUIDs and support forking/rebasing histories (per design).
- Terminal session infrastructure now spawns PTYs, maintains a VT buffer via `vte`, and streams colored snapshots into the renderer so panes display live glyphs (and ANSI art/image tests).
- `runtime-cli image-to-ansi` converts PNG/JPEG bitmaps into 24-bit colored ASCII so agents can pipe images directly into the TUI panes.
- `examples/tui-demos` streams ANSI art (image + spinning ray cube) to stress the renderer’s glyph/color path.
- `frame-feed` crate + `tui-demos feed-*` commands expose a shared memory RGB frame feed so external producers (e.g., Doom) can push native bitmaps into the renderer, with an accompanying input feed (toggle via `F9`) for routing keystrokes outside the PTY.
- Rigorous testing: unit tests plus eventual manual screenshot comparisons.
