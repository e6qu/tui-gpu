## Agent Tools

Agents can execute a curated set of tools through the runtime. Each tool call is logged as an event (for audit/replay) and subject to capability checks defined in the agent’s `BehaviorProfile`.

### Common constraints
- All tools execute within the project workspace (`$PROJECT_ROOT`). Paths must be relative and normalized; attempts to escape the workspace are rejected.
- Tool invocations are serialized into the event log with parameters and hashes of modified files (where applicable).
- Outputs are truncated/summarized if they exceed configured limits; full payloads can be stored as attachments when needed.

### `readfile`
- **Purpose**: Read text files inside the workspace.
- **Input**: `{ path: String, range: Optional<LineRange> }`.
- **Behavior**:
  - Validates path is inside workspace and file size below limit (configurable, default 1 MB).
  - Returns UTF-8 text; if binary or encoding fails, returns error.
  - Optionally supports line range to reduce payload.
- **Logging**: event stores file path + hash of content served.

### `patchfile`
- **Purpose**: Apply unified diffs to files in workspace.
- **Input**: `{ patches: String }` (git-style diff).
- **Behavior**:
  - Applies patches using `apply_patch` semantics.
  - Rejects changes that touch files outside workspace.
  - Conflicts or invalid diffs raise errors; partial application is rolled back atomically.
- **Logging**: record file list, diff hash, success/failure status.

### `curl`
- **Purpose**: Fetch remote HTTP resources and convert HTML to simplified Markdown.
- **Input**: `{ url: String, headers: Optional<Map>, method: Optional<String> }`.
- **Behavior**:
  - Performs HTTP(S) requests via sandboxed client (respecting network policy).
  - If response is HTML, run conversion pipeline:
    1. Parse HTML (e.g., `html5ever`).
    2. Strip scripts/styles.
    3. Convert to Markdown (e.g., `html2md`), simplifying tables/lists.
    4. Summarize if length > limit.
  - Non-HTML responses returned as-is (text) or encoded (binary).
- **Logging**: URL, status code, content hash; response body optionally attached.

### `manpages`
- **Purpose**: Query local manual pages for commands/APIs.
- **Input**: `{ topic: String, section: Optional<String> }`.
- **Behavior**:
  - Runs `man` command with sandboxed environment restricted to workspace (no external lookups).
  - Captures output, truncates to limit, converts ANSI escapes to Markdown.
  - Cache frequently requested topics to reduce overhead.
- **Logging**: topic/section requested.

### `bash`
- **Purpose**: Execute shell commands for build/test scripts.
- **Input**: `{ script: String, timeout: Duration }`.
- **Behavior**:
  - Runs `/bin/bash` with working directory = workspace root.
  - Environment sanitized (limited PATH, no network proxies unless allowed).
  - Shell is not permitted to `cd` outside workspace; enforcement via `chroot`/`pledge` equivalent or checking commands (TBD).
  - Output streamed back (stdout/stderr). Non-zero exit captured.
- **Logging**: script content hash, exit status, runtime metrics.

### Tool capability matrix
### `readonlybash`
- **Purpose**: Execute shell commands that are guaranteed not to mutate the workspace (e.g., `ls`, `grep`, `cat`).
- **Input**: `{ script: String, timeout: Duration }`.
- **Behavior**:
  - Uses `/bin/bash` with a restricted shell profile:
    - `set -o noclobber`
    - PATH limited to whitelisted binaries (`/bin`, `/usr/bin`)
    - `PROMPT_COMMAND` or shell wrapper checks each command against allowlist/denylist (no `rm`, `mv`, `chmod`, `>` redirects, `tee`, `nano`, `git commit`, etc.).
  - Filesystem mounted read-only if platform permits (e.g., using overlayfs/chroot). Otherwise commands are statically analyzed and rejected if they include write operations or redirections.
  - Prevents `cd` outside workspace; forbids functions or scripts that contain disallowed commands.
  - Output streamed back similarly to `bash`.
- **Logging**: script hash, sanitized command list, exit code.

### Capability matrix
| Tool         | Required capability                      |
|--------------|------------------------------------------|
| readfile     | `Capability::ReadWorkspace`              |
| patchfile    | `Capability::ModifyWorkspace`            |
| curl         | `Capability::NetworkAccess`              |
| manpages     | `Capability::AccessDocs`                 |
| bash         | `Capability::ExecuteScripts`             |
| readonlybash | `Capability::ExecuteReadOnlyScripts`     |

Agents without the necessary capability receive `Error::PermissionDenied`. Supervisors/librarians may have read-only subsets.

### Integration points
- API: expose `Command::InvokeTool { tool: ToolKind, payload: ToolPayload }`.
- Event log: every invocation emits `Event::ToolInvocation` with status/result summary.
- Replay: tool outputs can be rehydrated from attachments for deterministic runs.

Future tools can follow same pattern: define capability, sandboxing, logging.
