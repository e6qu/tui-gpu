## STATUS
- GUI renderer: wgpu surface + layout grid are back, the PTY output renders via the glyph pipeline, and the plasma/ray/Doom/YouTube demos now draw RGB textures inside the pane (plasma/ray support CPU or GPU compute). Doom automatically hooks the frame/input/audio feeds, and the YouTube demo now streams audio via yt-dlp/ffmpeg + rodio (toggle with `--youtube-audio`).
- TUI mode: terminal ANSI path is live again and the same demos stream frames as colored blocks (including GPU compute readbacks where available). F9 toggles PTY vs Doom input feed just like the GUI mode.
- `renderer-core` exposes `run_app_with_options` so binaries can embed the renderer and opt into FPS logging via `--fps-log`.
- Layout compiler + runtime core/CLI + docs are up-to-date.
