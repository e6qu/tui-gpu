## Renderer & GPU Pipeline Design

### Goals
- Maintain a dedicated GPU path (`wgpu` → Metal/Vulkan) from day one.
- Keep render latency below 5 ms for 200 Hz redraws while supporting rich overlays.
- Provide fallback transport (Kitty/WezTerm graphics) for SSH contexts.

### Layers & responsibilities
1. **Scene builder**
   - Input: layout rectangles + pane states from `designs/layout_and_accessibility.md`.
   - Output: per-layer `SceneLayer { id, z_index, region, glyph_requests[], overlay_primitives[] }`.
   - Maintains a diffable `SceneGraph` so we can skip encoding layers that remain unchanged.
2. **Glyph cache service**
   - Maintains `GlyphAtlasSet` (one atlas per font face/style). Each atlas has:
     ```
     struct GlyphAtlas {
         texture: wgpu::Texture,
         dimensions: (u32, u32),
         free_list: RangeAllocator,
         metadata: HashMap<GlyphId, AtlasEntry>,
     }
     ```
   - Upload path: glyph rasterizer → staging buffer (mapped) → `queue.write_texture`.
   - Publishes `GlyphHandle { atlas_index, uv_rect, msdf_params }` for use in shaders.
3. **Compute diff stage**
   - Buffers:
     ```
     struct Cell {
         char_code: u32,
         fg: PackedColor,
         bg: PackedColor,
         attrs: AttrFlags,
     }
     ```
   - SSBOs: `cells_prev`, `cells_curr`.
   - Compute shader algorithm:
     1. Each invocation handles `N` cells (configurable workgroup size 256).
     2. Compare `cells_prev[i]` vs `cells_curr[i]`; if different, atomically append `InstanceCmd` into per-layer ring buffer.
     3. Emit underline/selection commands based on `attrs`.
   - Feature flag allows falling back to CPU diff until shader is mature.
4. **Render passes**
   - Pipeline layout:
     ```
     set0 = uniforms (projection, time, cursor state)
     set1 = glyph atlas sampler array
     set2 = instance buffers (storage)
     ```
   - Pass order per frame:
     1. Background rectangles (solid fills).
     2. Text glyphs (instanced quads).
     3. Overlays/tooltips/menus.
     4. Optional post-processing (CRT, blur) executed only if enabled in layout metadata.
5. **Swapchains / presentation**
   - Desktop: one `wgpu::Surface` per window, present mode = mailbox/auto V-sync.
   - Headless/SSH: render to `wgpu::Texture`, copy to CPU buffer, encode via Kitty/WezTerm protocol with diffing on the CPU side.

### Resource & state specs
- **Instance ring buffer**
  ```
  struct InstanceBuffer {
      gpu_buffer: wgpu::Buffer,
      capacity: u32,   // number of instances
      write_offset: u32,
      fence_value: u64,
  }
  ```
  - Minimum capacity: `pane_cols * pane_rows` rounded to next power of two.
  - Resized lazily when `write_offset` would overflow; old buffer kept until GPU fence signals completion.
- **Uniform block**
  ```
  struct FrameUniforms {
      projection: mat4,
      time_ms: f32,
      cursor_pos: vec2,
      cursor_shape: u32,
      dpi_scale: f32,
  }
  ```
- **Shader constraints**
  - Textures use `TextureFormat::Rgba8UnormSrgb`.
  - MSDF sampling requires derivative support; we will generate both MSDF + fallback bitmap to support older GPUs.
  - Glyph quads rendered in clip space; coordinates computed from layout rectangles.

### Sequence diagram (desktop frame)
```
SceneBuilder::update() -> GlyphCache::request_glyphs()
GlyphCache -> wgpu queue (uploads as needed)
Renderer:
  - Map staging buffers for cells_prev/cells_curr
  - Dispatch compute shader (diff)
  - Encode render pass (background + glyphs + overlays)
  - Submit queue + present
Fence completion -> recycle instance buffers
```

### Data flow
```
Cell buffers (SSBO) ──> Compute diff ──> Instance ring buffers ──> Render pass ──> Swapchain / Kitty stream
Glyph requests ───────────────────────> Glyph cache ─────┘
```

### Performance considerations
- Persistent mapped buffers with subrange writes; avoid per-frame reallocations.
- Timeline semaphores/fences gate CPU writes until GPU completes.
- Partial presents (Wayland `wp_presentation`, CAMetalLayer region updates) minimize compositor load.
- Direct-to-scanout path (GBM/DRM, CAMetalLayer) reserved for kiosk use.
- Target budgets:
  - Glyph upload throughput: ≥ 500k glyphs/s sustained.
  - Frame submission time: ≤ 1 ms CPU, ≤ 4 ms GPU for 200×60 grid.
  - Instance buffer growth must remain <1.5× steady-state to limit VRAM churn.

### Testing hooks
- `wgpu` buffer read-backs validate compute diff output.  
- Frame samplers capture RGBA surfaces for golden comparisons.  
- Renderer exposes optional debug overlays showing dirty regions and GPU timings.
