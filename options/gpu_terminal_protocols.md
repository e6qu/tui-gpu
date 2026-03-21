## Option: GPU-capable terminal protocols (Kitty / WezTerm / iTerm2)

**Intent**: Keep running as a normal terminal program but exploit terminals that already render via GPU, using their pixel protocols (Kitty graphics, iTerm2 inline images, Sixel, future `wezterm` IPC). Best when SSH/TTY compatibility matters.

### Stack
- Capability detection via terminfo / environment (e.g., `KITTY_WINDOW_ID`, `WEZTERM_PANE`).
- Protocol adapters:
  - **Kitty graphics**: stream zlib-compressed RGBA tiles via OSC 1337 escapes, referencing server-side image IDs for reuse.
  - **iTerm2**: inline image protocol for PNG/JPEG data.
  - **Sixel** fallback for DEC-compatible terminals.
- CPU renders to texture (Skia, Cairo, or even headless `wgpu`), then blits via protocol.

### Strengths
- No extra executable/window; works over SSH as long as remote terminal supports the protocol.
- Terminal emulator already handles input, IME, DPI, selection, so app stays focused on rendering content.
- Easy progressive enhancement: fallback to ASCII art if protocol unavailable.

### Risks / open questions
- Throughput bounds: Kitty currently caps chunk sizes and can bottleneck on slower links; need delta compression to stay snappy.
- Limited control over vsync or color management; terminal decides composition.
- Coordinate systems vary; must manage origin, cell alignment, and scrollback semantics differently per emulator.

### Prototype focus
1. Implement Kitty protocol first (best tooling/docs), include server-side image reuse to minimize uploads.
2. Build capability negotiation layer (`TERM`, feature queries) with caching per session.
3. Benchmark streaming of 120 FPS small diffs vs. ASCII fallback over local + remote connections.
