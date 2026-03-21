## Project: Lapce (GPU-Accelerated Rust Code Editor)

### Stack and architecture
- Lapce's UI is powered by the Floem framework (vendored in the Lapce workspace), which offers wgpu-, Skia-, and TinySkia-based renderers. Production builds target the `vello`/wgpu backend.
- Documents use rope structures (xi-like) while Floem's layout engine (`taffy`) computes widget trees that eventually produce GPU draw lists.
- GPU resource acquisition is handled asynchronously (`floem/renderer/src/gpu_resources.rs`), allowing Lapce to spin up adapters/devices off the main thread.

### How Lapce interfaces with the GPU
- Floem's renderer requests a `wgpu::Device`/`Queue`, creates an sRGB swap chain per window, and records render passes using Vello/Vger (vector renderers built atop wgpu).
- Each frame, Lapce builds a scene graph of quads/text, then Vello tessellates + rasterizes them into wgpu storage/vertex buffers before issuing render passes.
- When GPU creation fails or the driver is missing features, Floem falls back to TinySkia (CPU path), keeping the editor functional albeit slower.

### Sample code (Floem GPU resource creation)
```rust
// floem/renderer/src/gpu_resources.rs:54-89
let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
    backends: Backends::from_env().or(backends).unwrap_or(Backends::all()),
    flags: InstanceFlags::from_env_or_default(),
    ..Default::default()
});
let (tx, rx) = sync_channel(1);
spawn({
    async move {
        let surface = match instance.create_surface(Arc::clone(&window)) { ... };
        let Ok(adapter) = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await else { ... };
        tx.send(
            adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: None,
                    required_features,
                    ..Default::default()
                })
                .await
                .map(|(device, queue)| Self { adapter, device, queue, instance })
                .map(|res| (res, surface)),
        ).unwrap();
    }
});
```
This is the same path Lapce runs before constructing Floem's renderer; it highlights how GPU setup happens asynchronously and surfaces a `wgpu::Surface` for the window.

### Notes for agent integrations
- Because Floem exposes declarative widgets, an agent can programmatically add overlay panels, inline completions, or visualization widgets that render directly through wgpu.
- Lapce already integrates LSP + terminal panes; GPU TUIs can inspect how Floem composes text and non-text layers to replicate similar responsive experiences.
- Floem's fallback renderers make it possible to test GPU concepts even on systems lacking Vulkan/Metal, while still sharing layout/logic with the GPU path.
