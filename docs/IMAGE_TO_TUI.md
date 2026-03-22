## Converting Images to Colored TUI Output

Use the `runtime-cli` helper to downsample PNG/JPEG images into colored ASCII art that can be streamed through the PTY and rendered inside the TUI panes.

```bash
cargo run -p runtime-cli -- image-to-ansi \
  --input assets/demo.png \
  --width 100 \
  > /tmp/demo.ansi
```

Key details:

- `--width` specifies the number of terminal columns to use. Height is automatically inferred from the image aspect ratio and our cell aspect (9×16 pixels), but you can override it via `--height`.
- `--palette` lets you provide a string of characters ordered from lightest to darkest (`" .:-=+*#%@"` by default). The converter maps pixel luminance to these glyphs and applies the exact RGB value through 24-bit ANSI sequences.
- The output is plain text, so you can pipe it into the renderer’s PTY (for example, `cat /tmp/demo.ansi` from inside the spawned shell) and the glyph pipeline will display the colored result.

This provides a straightforward path to render screenshots, bitmaps, or generated frames directly on the TUI surface without any additional GPU code. For live demos (including an animated ray-marched cube), see `examples/tui-demos` and run either the `image` or `cube` subcommands inside the PTY spawned by `render-app`.

### Streaming RGB Frames

When you want to render bitmaps natively (no ANSI degradation), use the shared frame feed backed by `frame-feed`. By default the renderer watches `/tmp/tui_gpu_framefeed` (override via `TUI_GPU_FRAME_FEED`). Any process can open that file, write RGB frames, and the renderer will blit them into the terminal pane.

Use the helper demos to populate the feed:

```bash
# stream a PNG into the feed, refreshing every 200 ms
cargo run -p tui-demos -- feed-image \
  --input assets/demo.png \
  --width 160 \
  --height 90 \
  --interval 200

# stream the ray-marched cube as raw RGB frames at 30 FPS
cargo run -p tui-demos -- feed-cube --width 160 --height 90 --fps 30
```

Future producers (e.g., a Doom wrapper) only need to write RGB data and bump the generation counter; `render-app` handles the rest.

#### Keyboard/event feed

The renderer can also forward keypresses to external producers through a companion input feed (`/tmp/tui_gpu_inputfeed`, configurable via `TUI_GPU_INPUT_FEED`). Press `F9` inside `render-app` to toggle between delivering keys to the PTY and to the input feed. Consumers read events with `InputFeedReader`—each event contains the UTF-8 bytes or escape sequences that would have been sent to the PTY (arrows, Home/End, etc.).
