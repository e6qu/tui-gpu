## Abstract API & Transports

### Canonical schema principles
- Strongly typed domain objects (Rust enums/structs). Serialization uses Serde with `tag`/`content` to maintain discriminants.
- Tagged unions for discriminated variants (e.g., `Event::AgentMessage`, `Command::SpawnSubagent`).
- No `null`/`Any`; optional values represented as `enum Optional<T> { Absent, Present(T) }` in JSON to avoid ambiguous `null`.
- IDs use prefixed strings (`agt_xxx`, `chn_xxx`, `evt_xxx`); schema enforces regex `[a-z]{3}_[A-Za-z0-9]{8}`.

### Surface areas
1. **Commands** (subset)
   ```
   enum Command {
       SpawnAgent { parent: Optional<AgentId>, role: AgentRole, capabilities: Vec<Capability> },
       SendMessage { channel: ChannelId, content: MessageContent, attachments: Vec<AttachmentRef> },
       ForkConversation { base_event: EventId, label: String },
       RebaseConversation { branch: BranchId, new_parent: EventId },
       ControlLayout { action: LayoutAction }, // e.g., FocusNode, ToggleOverlay
       ManageSignal { action: SignalAction },  // e.g., Raise, Clear
       RendererCommand { action: RendererAction }, // e.g., CaptureFrame
   }
   ```
2. **Events**
   ```
   enum Event {
       AgentMessage { agent: AgentId, channel: ChannelId, content: MessageContent },
       ModeChanged { agent: AgentId, from: AgentMode, to: AgentMode },
       ConversationForked { branch: BranchId, parent: EventId },
       SignalRaised { signal: Signal },
       InterruptChanged { ticket: InterruptTicket },
       PtyData { session: PtyId, chunk: Base64Bytes },
       LayoutFocusChanged { node: NodeId, method: FocusMethod },
       RendererFrameSample { frame_id: FrameId, storage_ref: FrameStoreRef },
       TransportLifecycle { transport: TransportKind, status: Connected|Disconnected, endpoint: String },
   }
   ```
3. **Queries/streams**
   - `Query::EventStream { mode: ReplayMode }`
   - `Query::Snapshot { scope: SnapshotScope }` (`Agents`, `Layout`, `Renderer`, `FullState`)
   - `Query::Metrics {}` returns counters/gauges.

### Transports
- **REST/HTTP**: simple POST/GET wrappers; commands encoded as JSON (schema enforced).  
- **WebSocket**: bidirectional streaming for interactive clients.  
- **Unix socket**: local JSON-RPC or Cap’n Proto for CLI tools.  
- **FIFO pipes**: scripting-friendly send/receive channels.  
- **CLI**: wrapper binary invoking commands, optionally reading from stdin for event streams.  
- **MCP (Model Context Protocol)**: adapter exposing the canonical schema to AI models/agents.

Each adapter performs only serialization/deserialization + auth; core logic remains transport-agnostic.

### Versioning
- Schema semver’d; transports include negotiated version to ensure compatibility.  
- Event log stores schema version per entry for replay safety.  
- Deprecations handled via additive fields + explicit migrations.
- Version negotiation:
  - REST: clients send `X-API-Version`; server responds `X-API-Version` + downgrade if necessary.
  - WebSocket: first message must be `{"type":"hello","version":"1.0.0"}`; server replies with accepted version.
  - CLI/FIFO: version passed via environment variable or initial handshake line (`VERSION 1.0.0`).
- Breaking changes require bumping major version and providing migration tooling for event logs (scripts to remap payloads).
