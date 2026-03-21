## High-Performance GPU TUI Architecture

### Mission
- Ship a GPU-first terminal-inspired UI (Metal/Vulkan via `wgpu`) with <5 ms latency.
- Treat DAG histories, forking/rebasing, and time-travel as first-class features for humans and agents.
- Support multi-agent swarms (subagents, channels, private messages, interrupts).
- Expose a strongly typed abstract API (multiple transports), event sourcing, structured logging, and full keyboard/menu control even in TUI mode.

### Core stack snapshot
- Rust runtime (`winit`, `wgpu`, HarfBuzz/`swash`, `libvterm`).
- Markdown/MDX templates compiled into deterministic layout trees.
- Renderer with Kitty/WezTerm fallback for SSH contexts.
- Headless mode that keeps the event log + API alive without the GPU surface.

### Component designs
The details for each subsystem live in dedicated design docs—refer to them instead of duplicating here:

| Domain | Design |
| --- | --- |
| GPU renderer, glyph cache, compute diff, performance levers | [`designs/renderer.md`](designs/renderer.md) |
| Markdown templates, layout engine, overlays, accessibility, menus | [`designs/layout_and_accessibility.md`](designs/layout_and_accessibility.md) |
| Agents, subagents (meeseeks), supervisors, librarians, DAG history, memory systems | [`designs/agents_and_history.md`](designs/agents_and_history.md) |
| Runtime/process model, event sourcing, headless/API mode, structured logging | [`designs/runtime_and_events.md`](designs/runtime_and_events.md) |
| Abstract API schema + REST/WebSocket/Unix/FIFO/CLI/MCP adapters | [`designs/api_and_transports.md`](designs/api_and_transports.md) |
| Tooling (readfile/patchfile/curl/manpages/bash) and capability gating | [`designs/tools.md`](designs/tools.md) |
| Testing pyramid (unit, integration, smoke E2E, future BDD/UI) | [`designs/testing_strategy.md`](designs/testing_strategy.md) |

### Guiding constraints
- GPU rendering, full DAG history tooling, subagent orchestration, and event sourcing must exist from day one.
- Markdown layouts are authoritative; runtime never mutates template structure.
- Single canonical schema defines all commands/events; transports are thin adapters.
- Menu/keyboard parity is mandatory regardless of render surface (GUI or TUI/terminal fallback).

### Implementation cadence (high level)
1. Stand up the baseline GPU renderer + glyph cache according to [`designs/renderer.md`](designs/renderer.md).
2. Integrate Markdown template compiler and accessibility plumbing per [`designs/layout_and_accessibility.md`](designs/layout_and_accessibility.md).
3. Implement event log + DAG storage + agent orchestration per [`designs/agents_and_history.md`](designs/agents_and_history.md) and [`designs/runtime_and_events.md`](designs/runtime_and_events.md).
4. Expose the canonical API over an initial transport (WebSocket) before layering additional adapters from [`designs/api_and_transports.md`](designs/api_and_transports.md).
5. Lock in unit/integration + smoke E2E coverage first; expand the rest of the pyramid per [`designs/testing_strategy.md`](designs/testing_strategy.md).

This document stays intentionally high-level; drill into the design files whenever you need mechanics, data flows, or API definitions.
