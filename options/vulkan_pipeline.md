## Option: Vulkan / `ash` (`vulkano`) explicit renderer

**Intent**: Go full Vulkan for maximum control over memory, synchronization, compute passes, and multi-queue scheduling—useful when TUIs evolve into ultra-dense dashboards that stream gigabytes of state per second.

### Stack
- Raw `ash` (C/C++) or `vulkano`/`erupt` (Rust) to drive device/queue management.
- `winit` or SDL for surface creation (MoltenVK on macOS, native Vulkan on Linux).
- Compute shader for cell diffing + glyph placement; fragment shader draws from large glyph atlas stored in device-local memory.
- Optional: timeline semaphores for overlapping async agent computations and rendering.

### Strengths
- Fine-grained control over memory allocations (bindless textures, descriptor indexing) to support thousands of glyph pages and textures (plots, previews).
- Predictable performance once tuned; can share GPU with CUDA/Metal compute by coordinating queue families.
- Enables multi-window or headless rendering paths (offscreen surfaces piped to video/stream).

### Risks / open questions
- Considerable boilerplate and maintenance (swap-chain recreation, descriptor set churn, validation layers).
- MoltenVK backend differs subtly from native Vulkan; debugging cross-platform bugs is costly.
- Fewer high-level text libraries; likely need custom glyph staging/packing.

### Prototype focus
1. Stand up a minimal `ash` renderer that displays glyph grid with timeline semaphore-based frame pacing.
2. Implement compute shader that consumes diff buffer and writes glyph vertex data directly, eliminating CPU loops.
3. Measure memory bandwidth + CPU usage on AMD/NVIDIA + Apple M-series to confirm benefits over `wgpu`.
