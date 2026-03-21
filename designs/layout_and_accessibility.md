## Layout, Templates & Accessibility

### Markdown/MDX Templates
- Source files live under `ui/*.mdx`. Naming convention: `screen_<feature>.mdx`.
- Supported syntax:
  - Standard Markdown headings/paragraphs for descriptive text.
  - MDX components drawn from a curated library (e.g., `<TerminalPane id="main" rows=40 cols=120/>`).
  - Frontmatter metadata (YAML) specifying screen ID, default hotkeys, access control.
  - Custom directives:
    ```
    :::overlay id="alerts" z=900 keyboard="alt+a"
    ...child components...
    :::
    ```
- Compiler steps:
  1. Parse Markdown/MDX into AST (`markdown-it` + custom MDX parser).
  2. Validate components against schema (no arbitrary JSX, no dynamic expressions).
  3. Produce `LayoutTree`:
     ```
     struct LayoutNode {
         id: String,
         component: ComponentKind,
         children: Vec<LayoutNode>,
         layout: LayoutProps,
         accessibility: AccessibilityProps,
     }
     ```
  4. Serialize to JSON + Rust constants for runtime (using `include_bytes!`).
- Determinism guarantees:
  - Compiler runs offline in build step; runtime only loads precompiled trees.
  - Templates hashed; event logs reference hash for replay validation.

### Layout Engine
- Engine uses `taffy` with nodes mirroring the `LayoutTree`.
- Layout props include:
  ```
  struct LayoutProps {
      display: Flex | Grid | Absolute,
      flex_direction: Row | Column,
      flex_grow: f32,
      basis: Dimension,
      padding: Edges,
      margin: Edges,
      min_size: Size,
      max_size: Size,
      z_index: i32,
  }
  ```
- Runtime pipeline:
  1. Load compiled `LayoutTree`.
  2. Inject live data (e.g., pane sizes) into nodes via property overrides.
  3. Call `taffy.compute_layout()` on root; capture rectangles.
  4. Emit `LayoutSnapshot { node_id, rect, z }` for scene builder.
- Re-layout triggers: window resize, DPI change, template switch, overlay add/remove. All triggers produce events so history/replay remains consistent.

### Overlays & z-index
- All layers (panes, menus, dialogs, tooltips) are assigned z-indices defined in the template; runtime enforces ordering.
- Overlay builder API lets agents spawn overlay nodes dynamically; they inherit keyboard navigation metadata.
- Overlay spec:
  ```
  struct OverlayDescriptor {
      overlay_id: String,
      parent_region: RegionId,
      z_offset: i32,
      focus_priority: u8,
      closable: bool,
  }
  ```
  - Overlays created via API command; runtime injects temporary nodes into layout tree before next layout pass.

### Menus & keyboard navigation
- UI must be fully operable without a mouse:
  - Each `LayoutNode` includes `AccessibilityProps { label, tab_index, mnemonic, role, aria_desc }`.
  - Focus manager maintains a doubly linked list of focusable nodes; Tab/Shift+Tab iterate, mnemonics jump directly.
  - "Press letter" hints rendered via overlay layer generated from the focus list.
- Menu specification:
  ```
  menu "File" {
      item "New Session" shortcut="Cmd+N" action=Command::CreateSession
      item "Headless Mode" shortcut="Cmd+Shift+H" action=Command::ToggleHeadless
  }
  ```
  - Templates can include `<MenuBar/>` component referencing menu definitions.
  - On macOS we mirror menu definitions to native AppKit menus; on other platforms we render GPU toolbar with identical semantics.
- Pointer input policy:
  - winit events captured, mapped to screen coordinates.
  - If event falls inside terminal pane AND the pane has mouse reporting enabled, we translate to libvterm sequences.
  - Otherwise we dispatch to overlay/menu components via hit-testing against layout rectangles.

### Accessibility instrumentation
- Each node includes IDs and semantic roles for screen readers/future automation.  
- Logging/event sourcing records focus changes to aid debugging.  
- Testing harness can walk the focus order headlessly to ensure coverage.
- Accessibility export format:
  ```
  {
      "node_id": "terminal.main",
      "role": "terminal",
      "label": "Main Terminal",
      "shortcut": "Ctrl+1",
      "bounds": [x, y, w, h],
      "parent": "root"
  }
  ```
  - Produced per frame (or on layout changes) to support external inspection tools.
