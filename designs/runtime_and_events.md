## Runtime, Event System & Processes

### Process topology
- Binary modes (CLI flags):
  - `--ui` (default): spawn renderer window(s), API server, PTY workers.
  - `--headless`: skip renderer, keep API/event log running.
  - `--replay <log>`: run replay engine without live inputs.
- Internal structure:
  ```
  [Main Runtime]  (tokio)
     ├─ Renderer task (if UI mode)
     ├─ Input manager (winit event loop)
     ├─ Layout/task coordinator
     ├─ API server (multiple transports)
     ├─ Event log writer
     ├─ PTY supervisor (spawns PTY workers per shell)
     ├─ Agent supervisor (spawns agent executors/subagents)
  ```
- Worker communication:
  - PTY workers send `PtyChunk` via bounded channel (size configurable; default 1024 messages).
  - Glyph cache uses shared-memory ring buffers for glyph bitmaps to avoid malloc churn.
  - Control signals travel over `tokio::broadcast` for fan-out (e.g., interrupts).

### Event bus
- All components publish/subscribe to strongly typed events (no `null`/`Any`), matching the canonical schema.
- Queues support priorities; interrupts/signals preempt regular traffic.
- Event sourcing layer records every event plus optional frame samples for debugging.
- Event dispatcher:
  ```
  struct EventBus {
      normal_tx: mpsc::Sender<Event>,
      priority_tx: mpsc::Sender<Event>,
      subscribers: Vec<Subscriber>,
  }
  ```
  - Priority queue drained before normal queue each tick.
  - Subscribers declare filters (e.g., `EventKind::AgentMessage`) to reduce traffic.

### Event log storage
- On-disk format (per day):
  - `events/YYYY-MM-DD.log` (binary or JSONL depending on feature flag). Each entry includes event struct plus CRC.
  - `snapshots/YYYY-MM-DD/HHMMSS.snapshot` storing serialized state (agent registry, layout snapshot, renderer state).
- Metadata index (`events/index.sqlite`):
  - Tables: `events(event_id TEXT PRIMARY KEY, file TEXT, offset INTEGER)`.
  - Allows O(1) lookup for replay/time-travel.
- Structured logs (human readable) default to JSONL:
  ```
  {"ts":"...","level":"INFO","event_id":"evt_123","message":"agent mode change","details":{...}}
  ```

### Headless/API integration
- Abstract API surface defined once (tagged unions, literal enums).  
- Transports (REST, WebSocket, Unix socket, FIFO, CLI, MCP) adapt the same schema.  
- Headless mode uses API + renderer off-switch; still processes events identically for deterministic replay.
- API server architecture:
  - Core router exposes async functions `handle_command(Command)` and `subscribe(StreamRequest) -> Stream`.
  - Transports wrap router: e.g., WebSocket adapter handles handshake, then forwards JSON-serialized commands/events.
  - CLI adapter communicates via stdio using the same schema encoded as JSON lines.

### Monitoring & resilience
- Supervisors restart failed workers; restarts logged as events.
- Timeline semaphores/fences report GPU backpressure, raising interrupts if render loop starves.
- Metrics (latency, queue depth) exported via structured logs or optional Prometheus endpoint.
- Health probes:
  - `/healthz` HTTP endpoint (for REST server) returns summary: renderer status, event log lag, PTY worker count.
  - CLI command `status` prints same data.
- Backpressure handling:
  - If event bus queue exceeds threshold, emit `Event::Backpressure` and shed low-priority updates (e.g., frame samples) until recovered.
  - GPU fence timeouts raise `Interrupt::RendererStall`.
