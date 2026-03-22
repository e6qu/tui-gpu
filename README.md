# tui-gpu

Prototype GPU-accelerated terminal renderer with shared frame/input feeds for driving external producers (ANSI demos, DoomGeneric, etc.).

## Run the renderer

Pick a demo with `--demo`:

| Demo        | Description                                                     |
|-------------|-----------------------------------------------------------------|
| `terminal`  | Default PTY-only terminal.                                      |
| `plasma`    | Built-in RGB plasma animation (use the GPU window or `--mode tui`). |
| `ray`       | CPU/GPU raytracer demo rendered into the pane (works in GUI and TUI). |
| `doom`      | Streams DoomGeneric frames; requires `--doom-iwad /path/to/wad`. |
| `youtube`   | Streams a YouTube video (requires `yt-dlp` + `ffmpeg`).               |

```bash
# default VT-only demo
cargo run -p render-app

# plasma RGB demo in the GUI window (CPU generator)
cargo run -p render-app -- --demo plasma --compute cpu

# GPU compute + GUI output
cargo run -p render-app -- --demo plasma --compute gpu --mode gui

# render the same demo inside the ANSI/TUI mode (CPU path)
cargo run -p render-app -- --demo plasma --mode tui

# GPU compute feeding ANSI output (downsampled)
cargo run -p render-app -- --demo plasma --compute gpu --mode tui

# raytracer demo (GUI or TUI)
cargo run -p render-app -- --demo ray
cargo run -p render-app -- --demo ray --mode tui

# log average FPS every 5 seconds
cargo run -p render-app -- --demo plasma --mode gui --fps-log 5

# youtube demo (GUI/TUI, CPU pipeline; configure via env vars)
export TUI_GPU_YOUTUBE_URL="https://www.youtube.com/watch?v=dQw4w9WgXcQ"
cargo run -p render-app -- --demo youtube
cargo run -p render-app -- --demo youtube --mode tui
# disable audio playback if you only want the video feed
cargo run -p render-app -- --demo youtube --youtube-audio false
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

## Renderer library

The shared rendering logic now lives in the `renderer-core` crate. It exposes `run_gui` (GPU window), `run_tui` (ANSI), and `run_app` helpers so other binaries can embed the renderer without reimplementing the event loops. Every demo can pick the compute backend (`ComputeMode::Cpu` or `ComputeMode::Gpu`) independently from the presentation backend (`AppMode::Gui` for pixel output or `AppMode::Tui` for ANSI).

```rust
use renderer_core::{run_app_with_options, AppMode, ComputeMode, DemoKind, RendererOptions};
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    // Launch the ray demo in the GPU window, log FPS every 5 seconds.
    run_app_with_options(
        DemoKind::Ray,
        ComputeMode::Gpu,
        AppMode::Gui,
        RendererOptions {
            fps_sample_interval: Some(Duration::from_secs(5)),
        },
    )
}
```

The same API can render CPU-style colored ANSI output by swapping to `AppMode::Tui` and choosing either CPU or GPU compute. Examples and future binaries can call into `renderer-core` directly to mix PTY glyph rendering with RGB textures in whichever combination they need.

> GPU compute is currently available for the `plasma` and `ray` demos. The terminal/PTY demo and Doom feed continue to run in CPU mode.

### YouTube demo

The built-in YouTube demo relies on `yt-dlp` and `ffmpeg` to fetch and decode frames. Install them first (reuse the same steps listed in the `youtube-ansi` helper below). Configure playback via environment variables (all optional):

| Variable | Description | Default |
|----------|-------------|---------|
| `TUI_GPU_YOUTUBE_URL` | YouTube URL to stream. Ignored if `TUI_GPU_YOUTUBE_INPUT` is set. | `https://www.youtube.com/watch?v=dQw4w9WgXcQ` |
| `TUI_GPU_YOUTUBE_INPUT` | Local video file to play instead of streaming. | unset |
| `TUI_GPU_YOUTUBE_WIDTH` / `TUI_GPU_YOUTUBE_HEIGHT` | Target decode resolution. Height defaults to a 16:9 aspect ratio when omitted. | `320x180` |
| `TUI_GPU_YOUTUBE_FPS` | Frame rate requested from ffmpeg. | `24` |
| `TUI_GPU_YOUTUBE_YTDLP` / `TUI_GPU_YOUTUBE_FFMPEG` | Paths to the executables. | `yt-dlp` / `ffmpeg` |
| `TUI_GPU_YOUTUBE_FORMAT` | yt-dlp format selector for the video stream. | AVC/H.264 MP4 preference |
| `TUI_GPU_YOUTUBE_AUDIO` | Set to `false` to disable audio playback (CLI flag `--youtube-audio`). | `true` |
| `TUI_GPU_YOUTUBE_AUDIO_FORMAT` | yt-dlp format selector for the audio stream. | `bestaudio/best` |
| `TUI_GPU_YOUTUBE_AUDIO_RATE` | Sample rate (Hz) for audio playback. | `44100` |

Run `render-app -- --demo youtube` (GUI) or add `--mode tui` for the ANSI view. Compute mode is always CPU for this demo. Use `--youtube-audio false` or `TUI_GPU_YOUTUBE_AUDIO=false` if you only want the silent video feed.

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

The renderer opens the shared audio/input feeds automatically when you pick `--demo doom`: audio events are mixed via `rodio`, and keyboard focus starts on the external feed (press `F9` to fall back to the PTY shell).

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
