## TUI Demos

The `examples` folder collects standalone demos that exercise the renderer’s VT/glyph pipeline.

### `tui-demos`

A small CLI with two subcommands:

- `image`: converts a PNG/JPEG into 24‑bit ANSI output (using the shared `ansi-image` crate) so you can `cat` the result inside the renderer pane.
- `cube`: generates an animated, ray-marched cube rendered in ANSI color—pipe its output into the PTY to stress-test glyph throughput.
- `feed-image`: loads a PNG/JPEG, rescales it, and continuously writes raw RGB frames into the shared frame feed (`/tmp/tui_gpu_framefeed` by default). The renderer consumes these frames as native textures.
- `feed-cube`: streams the same cube animation as raw RGB frames, ideal for exercising the GPU blit pipeline.

Run them with:

```bash
cargo run -p tui-demos -- image --input assets/demo.png --width 120 | cat
cargo run -p tui-demos -- cube --width 120 --height 48 --frames 600 --sleep 16 | cat
cargo run -p tui-demos -- feed-image --input assets/demo.png --width 160 --height 90 --interval 200
cargo run -p tui-demos -- feed-cube --width 160 --height 90 --fps 30
```

Both commands stream plain text, so you can launch them inside the PTY spawned by `render-app` (or redirect into a file and replay later).
The `feed-*` commands write directly into the shared frame feed; run them alongside `render-app` to see native RGB content inside the GPU pane. Press `F9` in `render-app` to toggle keyboard focus between the terminal PTY and the input feed (so Doom or other external producers can receive keys).

### `doom-feed`

Build the full DoomGeneric engine into a feed producer. Provide an IWAD (Doom/Freedoom) and it will stream RGB frames and listen to the shared input feed:

```bash
cargo run -p doom-feed -- --wad /path/to/freedoom1.wad
```

The renderer (with `F9` toggled to external) will display the Doom output while feeding keystrokes (arrows, WASD, etc.) to the game.
