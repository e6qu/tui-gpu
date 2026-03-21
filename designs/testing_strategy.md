## Testing & QA Strategy

### Pyramid overview (with targets)
1. **Unit tests** (coverage ≥ 80% for core crates)
   - Renderer math (matrix ops, glyph packing, MSDF sampling)
   - Event serialization/deserialization round-trips for every schema variant
   - Markdown template compiler (AST validation, layout tree generation)
   - Agent command routing, DAG manipulation (fork/rebase invariants)
   - Runtime utilities (ring buffers, timeline semaphores wrappers)
2. **Integration tests**
   - PTY → libvterm → renderer path using headless `wgpu` backend; assert on buffer contents.
   - API loopback: issue commands via WebSocket adapter, verify events/side effects.
   - Event replay equivalence: record short session, replay, compare final snapshots.
   - Glyph cache stress: simulate large font sets to ensure eviction/resizing works.
3. **Smoke E2E** (CI-blocking)
   - Script: launch app headless, spawn plan/build agents, send message, fork conversation, verify API output.
   - Duration ≤ 60 s to keep CI fast.
4. **Acceptance/BDD (post-MVP)**
   - Define `.feature` files (Gherkin) executed via CLI adapter and headless renderer.
   - Steps interact using canonical API commands; assertions inspect event logs.
5. **Full UI tests (post-MVP)**
   - tmux/expect or `xvfb`-driven automation to simulate keyboard/pointer flows.
   - Frame sampling: capture frames, compare to stored PNGs within tolerance (perceptual diff).

### Tooling & infrastructure
- Test harness uses `cargo nextest` for unit/integration speed; `cargo tarpaulin` (or `grcov`) for coverage reports.
- Provide fixture data under `tests/data/`:
  - `layouts/` compiled templates
  - `events/` sample logs
  - `glyphs/` reference atlases for golden comparisons
- Deterministic RNG seeds stored alongside test cases to ensure reproducibility.
- Benchmark suite (Criterion) tracks:
  - Renderer frame encode time
  - Compute diff throughput
  - Event replay speed (events/sec)

### CI considerations
- Matrix:
  - Linux (Vulkan), macOS (Metal), Windows (DX12) for renderer tests.
  - CPU-only fallback using `wgpu` headless for quick checks.
- Stages:
  1. Lint + unit
  2. Integration
  3. Smoke E2E
  4. (Optional) Benchmark/perf on nightly schedule
- Logs:
  - Structured logs captured as artifacts when tests fail.
  - Renderer frame dumps stored for failed GPU tests to aid debugging.
