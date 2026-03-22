# tui-gpu

Prototype GPU-accelerated terminal renderer with shared frame/input feeds for driving external producers (ANSI demos, DoomGeneric, etc.).

## Run the renderer

Pick a demo with `--demo`:

| Demo        | Description                                                     |
|-------------|-----------------------------------------------------------------|
| `terminal`  | Default PTY-only terminal.                                      |
| `plasma`    | Built-in RGB plasma animation (use the GPU window or `--mode tui`). |
| `ray`       | CPU raytracer demo rendered into the pane (works in GUI and TUI). |
| `doom`      | Streams DoomGeneric frames; requires `--doom-iwad /path/to/wad`. |

```bash
# default VT-only demo
cargo run -p render-app

# plasma RGB demo in the GUI window
cargo run -p render-app -- --demo plasma

# render the same demo inside the ANSI/TUI mode
cargo run -p render-app -- --demo plasma --mode tui

# raytracer demo (GUI or TUI)
cargo run -p render-app -- --demo ray
cargo run -p render-app -- --demo ray --mode tui
```

The window displays the current layout (menus + terminal pane). By default the pane renders VT glyphs sourced from the PTY (a shell). When you pick a demo that streams RGB frames (e.g. `plasma` or `doom`), the pane automatically switches to textured output. Press `F9` at any time to toggle keyboard focus between the shell (VT mode) and the external input feed (used by raw RGB producers such as Doom).

Running headless with `--mode tui` (legacy `--tui` still works)? Add `--tui-sample-ms 2000` (or any millisecond interval) to down-sample Doom’s frames—ideal for remote control over `tmux send-keys` while you capture screenshots.

## Stream ANSI demos

The `tui-demos` binary exposes text-based demos you can run inside the PTY. The repo ships with a sample image at `assets/demo.png`:

```bash
# convert an image to ANSI and pipe it inside the PTY
cargo run -p tui-demos -- image --input assets/demo.png --width 120 | cat

# animated cube using ANSI glyphs
cargo run -p tui-demos -- cube --width 120 --height 48 --frames 600 --sleep 16 | cat
```

## Stream raw RGB frames

`render-app` watches a shared-memory frame feed located at `/tmp/tui_gpu_framefeed` (override with `TUI_GPU_FRAME_FEED`). Any process can write RGB frames into this feed and the renderer will display them directly in the pane. Use the helper commands:

```bash
# continuously stream a PNG/JPEG as RGB frames (interval in ms)
cargo run -p tui-demos -- feed-image \
  --input assets/demo.png \
  --width 160 \
  --height 90 \
  --interval 200

# stream the ray-marched cube at 30 FPS
cargo run -p tui-demos -- feed-cube --width 160 --height 90 --fps 30
```

While frames are flowing, the pane automatically switches from glyph rendering to textured RGB output. Press `F9` to flip keyboard focus between the PTY and the external input feed (`/tmp/tui_gpu_inputfeed`, configurable via `TUI_GPU_INPUT_FEED`). External producers should read the input feed to receive key events.

## Running Doom

`render-app` can now spawn the DoomGeneric feed internally. Provide an IWAD (`doom1.wad`, `freedoom1.wad`, etc.) and run the `doom` demo:

```bash
cargo run -p render-app -- --demo doom --doom-iwad assets/freedoom1.wad --mode gui
# or terminal-only mode:
cargo run -p render-app -- --demo doom --doom-iwad assets/freedoom1.wad --mode tui
```

#### Quick start with Freedoom

Download a free IWAD (Freedoom) directly into the repo and run Doom without extra setup:

```bash
curl -L https://github.com/freedoom/freedoom/releases/download/v0.13.0/freedoom-0.13.0.zip -o assets/freedoom.zip && \
unzip -j assets/freedoom.zip freedoom-0.13.0/freedoom1.wad -d assets && \
rm assets/freedoom.zip && \
cargo run -p render-app -- --demo doom --doom-iwad assets/freedoom1.wad --mode gui
# or for text-mode:
cargo run -p render-app -- --demo doom --doom-iwad assets/freedoom1.wad --mode tui
```

Steps:

1. Start `render-app` with `--demo doom --doom-iwad …` as shown above.
2. When running in the GPU window without Doom, press `F9` once to switch keyboard routing to the external feed. With the Doom demo, the renderer starts in external-input mode automatically. In `--mode tui`, keystrokes are forwarded directly and `q` exits.
3. Doom’s framebuffer streams into the pane (or the ANSI terminal), and your keystrokes (WASD, arrows, space, etc.) are routed through the feed.
4. Press `F9` again to return focus to the PTY or hit `Esc`/`q` to exit.

This setup lets you experiment with GPU blitting, native RGB rendering, and off-the-shelf engines (like Doom) without touching the renderer’s code—just stream frames/events through the feeds.

## Colored-ANSI YouTube demo

The `youtube-ansi` helper streams a YouTube video (via `yt-dlp` + `ffmpeg`) and paints frames as colored Unicode glyphs in your terminal (with synchronized audio playback).

Install the prerequisites:

```bash
# macOS
brew install yt-dlp ffmpeg

# Ubuntu/Debian
sudo apt-get update && sudo apt-get install -y yt-dlp ffmpeg
```

Then run the demo (replace the URL if you prefer a different video):

```bash
cargo run -p youtube-ansi -- --url https://www.youtube.com/watch?v=dQw4w9WgXcQ --width 96
```

Use `--input path/to/video.mp4` to render a local file instead. `--width`, `--height`, and `--fps` control the ANSI output. Pipe the output into the PTY that `render-app` spawns (or run it directly in your shell) to watch videos rendered entirely with ANSI color. Audio is streamed alongside the video—pass `--no-audio` if you just want silent playback, or tweak `--audio-format` (default `bestaudio/best`) to match the available tracks.

Frames now default to block-shading glyphs (`█ ▀ ▄`) for high contrast. Prefer even denser output? Switch to Unicode braille with `--glyph-mode braille` (8 subpixels per char) or the legacy ASCII renderer with `--glyph-mode palette`. When using palette mode you can pick from curated ramp presets (`--ascii-preset classic|dense`) or supply your own glyph string via `--ascii-palette " .:-=+*#%@"`.

Under the hood the command asks `yt-dlp` for a direct video stream URL (via `--get-url`) and hands it to ffmpeg for decoding. The default `--video-format` prefers MP4/H.264 (`bestvideo[ext=mp4][vcodec^=avc1]/bestvideo[ext=mp4]/bestvideo`), but you can override it to fit whatever a channel publishes. If the default fails, broaden it:

```bash
cargo run -p youtube-ansi -- \
  --url https://www.youtube.com/watch?v=dQw4w9WgXcQ \
  --width 96 \
  --format "bestvideo[ext=mp4]+bestaudio[ext=m4a]/bestvideo[ext=webm]+bestaudio/best"
```

The renderer only needs the video URL, so we take the first line from `yt-dlp --get-url` (and separately request the audio URL if audio is enabled). Run `yt-dlp --list-formats URL` to inspect what’s available, and feel free to pass `--video-format bestvideo+bestaudio/best` to force the highest-resolution VP9/AV1 stream—ffmpeg handles those just fine before converting frames into ANSI.

## Toward a GPU-rendered TUI library

We’ve started carving out clean building blocks (layout compiler, glyph/text cache, PTY buffer, shared frame/input feeds, GUI/TUI launchers). See `docs/GPU_TUI_BUILDING_BLOCKS.md` for the roadmap that turns these pieces into a reusable GPU/GPGPU-driven TUI foundation.
