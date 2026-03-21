## Agents, Subagents & Conversation History

### Agent model
- Agents described by:
  ```
  struct Agent {
      id: AgentId,
      role: AgentRole,          // plan | build | review | monitor | supervisor | librarian | custom
      mode: AgentMode,          // editing | observing | paused | terminated
      capabilities: BitFlags<Capability>,
      parent: Option<AgentId>,  // None for roots, Some(parent) for subagents
      children: Vec<AgentId>,   // populated for root agents; subagents keep empty vec
      channels: Vec<ChannelId>,
  }
  ```
- Mode transitions allowed graph (edges represent permitted changes):
  ```
  plan <-> build
  plan <-> review
  build <-> monitor
  review -> plan (but not vice versa without privilege)
  ```
  Each transition emits `Event::AgentModeChanged`.
- Channel taxonomy:
  - `Channel::Public { topic }`
  - `Channel::Private { participants: Vec<AgentId> }`
  - `Channel::Swarm { swarm_id, topic }`
  - Channels have ACLs enforced by the controller.
- Behavior configuration:
  - Each agent loads a `BehaviorProfile` at creation:
    ```
    struct BehaviorProfile {
        config_id: String,
        settings: BehaviorSettings,   // deserialized from TOML/YAML
        embedded_code_ref: Option<CodeArtifactId>, // optional script/program
    }
    struct BehaviorSettings {
        risk_tolerance: f32,
        retry_strategy: RetryConfig,
        escalation_policy: EscalationConfig,
        allowed_channels: Vec<ChannelId>,
        custom_params: HashMap<String, Value>,
    }
    ```
  - Configuration sources:
    1. Static config files checked into repo (e.g., `configs/agents/plan.toml`).
    2. Runtime overrides via API (`Command::UpdateBehavior { agent, settings }`).
  - Embedded code:
    - Placeholder for future plugin system (e.g., WASM modules or embedded scripting). Documented as `CodeArtifact` references that can be loaded/executed to customize decision logic.
    - Security model TBD; initial implementation may sandbox to a limited VM.
  - Behavior changes emit events for audit; meeseeks inherit a copy of parent profile but may override specific knobs (timeouts, aggression levels).
- Memory system:
  - Each agent owns two memory stores:
    1. **Short-term memory (STM)**: in-memory ring buffer of recent observations/events.
       ```
       struct ShortTermMemory {
           capacity: usize, // default 256 entries
           entries: VecDeque<MemoryEntry>,
       }
       struct MemoryEntry {
           timestamp: DateTime<Utc>,
           event_id: EventId,
           summary: String,
           importance: f32,
       }
       ```
       - Used for immediate context when generating responses or decisions.
       - Eviction policy: drop lowest-importance entries when capacity exceeded.
    2. **Long-term memory (LTM)**: persisted store grouped by topic/key/embeddings.
       ```
       struct LongTermMemory {
           topic_index: HashMap<TopicLabel, Vec<MemoryRecordId>>,
           keyword_index: HashMap<String, Vec<MemoryRecordId>>,
           embedding_store: VectorIndex, // e.g., approximate NN search
       }
       struct MemoryRecord {
           id: MemoryRecordId,
           created_at: DateTime<Utc>,
           topic: TopicLabel,
           keywords: Vec<String>,
           embedding: Vec<f32>,
           content: String,
       }
       ```
       - Records created via API (`Command::StoreMemory`) or automatically when events flagged as important.
       - Semantic search: queries hashed to embeddings; vector index returns nearest records.
  - Long-term storage organization:
    - Filesystem-based CAS similar to conversation graph, or dedicated KV store (`ltm/topic/<label>/<record>.json`).
    - Metadata includes access control (who can read/use memory).
    - Agents can reference their LTM when planning; meeseeks typically only use parent’s relevant LTM subset.
- Agent spawning rules:
  - Only root agents (parent = None), controller processes, or supervisor agents may call `Command::SpawnAgent`.
  - Subagents cannot spawn additional agents; attempting to do so returns `Error::PermissionDenied`.
  - Parent-child relationships maintained in both directions for auditing; on subagent termination, parent notified via event.
- **Meeseeks subagents**:
  - Subagents spawned to solve a single task/problem are designated as “meeseeks”.
  - Metadata flag `AgentKind::Meeseeks` indicates they must self-terminate once the assigned task completes or timeout elapses.
  - Meeseeks inherit minimal capabilities necessary for the task and cannot fork new sessions or spawn children.
  - API surfaces `Command::SpawnMeeseeks { parent, task_description, timeout }` so agents can explicitly request these single-shot helpers.
- **Librarian agents**:
  - Role `librarian` with capability `Capability::CurateKnowledge`.
  - Responsibilities:
    - Maintain curated views into long-term memory (topic-specific catalogs).
    - Expose information to other agents via controlled channels (e.g., `Command::RequestKnowledge { topic, scope }`).
    - Enforce access policies: check requester permissions before returning memory records or conversation histories.
  - Librarians can subscribe to memory write events; they decide which records become publicly searchable vs. restricted.
  - When responding, librarians create `Event::KnowledgeShare` linked to the requesting agent and memory record IDs, ensuring auditability.
- Supervisor agents:
  - Role `supervisor` with capability `Capability::ManageAgents`.
  - Semi-ephemeral: spawned on demand to monitor stuck agents; expected to terminate after resolving issues.
  - Monitor agent activity (lack of events, repeated mode flapping) via heuristics. If thresholds exceeded, emit `Signal::AgentStalled` and optionally issue `Command::TerminateAgent`.
  - All supervisor actions logged (who terminated whom, reason, timestamps). Supervisors cannot spawn subagents of their own; they act only as overseers.
- Agent sessions:
  ```
  struct AgentSession {
      session_id: SessionId,        // UUIDv4 string (ses_xxxx)
      agent: AgentId,
      started_at: DateTime<Utc>,
      branch: BranchId,
      status: SessionStatus,
  }
  enum SessionStatus { Active, Paused, Completed, Aborted }
  ```
  - Every command or message references both `agent` and `session_id`, allowing multiple concurrent sessions per agent.
  - Sessions can fork/rebase similar to branches: `Command::ForkSession { session_id, new_branch }`, `Command::RebaseSession { session_id, onto_branch }`.
  - Session metadata recorded in event log for replay; editing a session rewrites its branch pointers but retains original events for audit.

### Event sourcing & DAG history
- Event struct:
  ```
  struct Event {
      id: EventId,
      parents: SmallVec<[EventId; 2]>,   // typically 1, >1 for merges
      timestamp: DateTime<Utc>,
      actor: ActorId,                    // agent or user
      payload: EventPayload,
      schema_version: u16,
      signature: Option<Signature>,      // optional authenticity check
  }
  ```
- `EventPayload` variants include `AgentMessage`, `ModeChange`, `Signal`, `Interrupt`, `LayoutFocusChange`, `RendererFrameSampleRef`, etc. The `AgentMessage` variant carries its own UTC timestamp (in addition to the event header) so forwarded/replayed messages preserve the original send time:
  ```
  enum EventPayload {
      AgentMessage {
          message_id: MessageId,
          sent_at: DateTime<Utc>,
          channel: ChannelId,
          author: AgentId,
          session: SessionId,
          content: MessageContent,
          labels: Vec<TopicLabel>,
      },
      ...
  }
  ```
- Forking semantics:
  - Creating a new branch adds `Event` with parent = base event, metadata `branch_id`.
  - Rebasing replays events on new parent; original events remain but get `metadata.rebased_to`.
  - Time-travel uses replay engine to apply events up to chosen `EventId`.
- Storage:
  - Events stored in append-only log segmented by day.
  - Conversation graph persisted in a lightweight content-addressed store:
    - Each `GraphNode` serialized to bytes, hashed (SHA-256), and stored under `.graph/objects/ab/cd...`.
    - `GraphEdge` objects stored similarly, referencing node hashes.
  - Conversation "heads" map `branch_id -> node_hash`, enabling cheap forks/rebases by updating head pointers. Session branches follow the same mechanism (each session maintains its own branch pointer referencing conversation objects).
    - CAS format mirrors git objects (header `type length\0payload`) for familiarity and tooling reuse.
  - Indexes: `agent_id -> Vec<EventId>`, `channel_id -> Vec<EventId>`, `branch_id`.
  - Replay engine rehydrates graphs by streaming objects from CAS, ensuring deterministic visualization.

### Signals & interrupts
- Agents can emit signals (e.g., “need review”, “pause build”) routed through the event bus.
- Interrupts are high-priority events that can suspend/resume tasks; overlays visualize active interrupts.
- Signal payload:
  ```
  struct Signal {
      kind: SignalKind,  // e.g., NeedReview, ResourceWait, BuildFailed
      target: SignalTarget::Agent(AgentId) | ::Channel(ChannelId) | ::Swarm(SwarmId),
      severity: Severity::Info|Warn|Error,
      expires_at: Option<DateTime>,
  }
  ```
- Interrupt states tracked via `InterruptTicket { id, owner, reason, status }`; status transitions (Pending -> Active -> Cleared) logged.

### Conversation explorer screens
- Templates include dedicated views for:
  - Graph visualization of conversation DAGs.
  - Swarm dashboards showing concurrent threads.
  - Private message inboxes/outboxes.
- Users can fork, merge, or annotate histories via keyboard/mouse; agents access the same operations via API.
- Graph data served by API endpoint returning:
  ```
  struct ConversationGraph {
      nodes: Vec<GraphNode>,
      edges: Vec<GraphEdge>,
  }
  struct GraphNode { event_id, type, label, status }
  struct GraphEdge { from, to, relation }   // relation = fork|merge|response
  ```
- Metadata:
  - Each conversation maintains `ConversationMeta`:
    ```
    struct ConversationMeta {
        conversation_id: ChannelId,
        created_at: DateTime<Utc>,
        created_by: ActorId,
        builtin_topic: BuiltinTopic,        // e.g., Build, Plan, Incident
        agent_labels: Vec<TopicLabel>,      // agent-supplied tags
        user_labels: Vec<TopicLabel>,
        description: Optional<String>,
    }
    ```
  - Commands:
    - `LabelConversation { conversation_id, label: TopicLabel, scope: Agent|User }`
    - `UpdateConversationMeta { description, builtin_topic }`
    - `LabelMessage { message_id, label: TopicLabel }` (allows tagging individual messages for classification)
  - Labels stored alongside CAS nodes for search/filter; UI surfaces topic chips.

### Persistence & replay
- Event log is append-only with periodic snapshots to speed restore.
- Structured logging ties each log entry to event IDs for correlation.
- Replay engine can feed recorded events back into the system (UI, agents, renderer) for debugging or automated regression tests.
- Snapshot cadence: every 5 minutes or 500 events per channel, whichever comes first.
- Replay API:
  ```
  enum ReplayMode { LiveFollow, Until(EventId), Range { start, end } }
  ```
  - Returns deterministic sequence of events referencing layout hashes, renderer frame IDs, etc.
