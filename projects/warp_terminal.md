## Project: Warp (Rust GPU Terminal)

### Stack and architecture
- Warp is closed-source, but public talks/blogs confirm it is written in Rust with a GPU scene graph (internally named GPUI) layered atop `wgpu`. It renders the terminal as structured blocks rather than raw escape sequences.
- Each command block is treated as a React-like component with diffing; GPU renderer draws text + UI chrome (prompts, inline completions, AI responses) as part of the same pipeline.
- macOS build uses Metal through `wgpu`; Linux beta uses Vulkan.

### GPU considerations (based on public info)
- Structured blocks allow Warp to keep per-block dirty rectangles, so the GPU only redraws parts of the command history that change (e.g., focus ring, AI inline answers).
- Inline Markdown renders are precomputed (syntax-highlighted) then uploaded as GPU textures so they can animate (expand/collapse) with the rest of the scene.
- Warp ships inline AI overlays that share the same GPU pipeline, mixing text and vector UI.

### Sample code availability
- The renderer is not open-source, so no direct code snippets are available for inspection. However, the architecture mirrors Zed’s open-source GPUI (see `third_party/zed`) since the teams share lineage; refer to `crates/gpui_wgpu` for representative code.

### Notes for agent workflows
- Warp already integrates AI command search; it is a reference for how to present agent suggestions inline with terminal output while retaining GPU performance.
- Block-based terminal output may inspire GPU TUIs to treat command results as structured nodes instead of raw glyph grids, simplifying interactive transformations.
- Because Warp is proprietary and cloud-connected, extending it for bespoke agents requires using Warp's APIs rather than modifying the renderer.
