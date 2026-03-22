## GAP ANALYSIS
- GUI renderer + TUI now render the PTY and RGB demos again. Plasma/ray support both CPU and GPU compute, YouTube runs via CPU decoding, and Doom uses the shared input/audio feeds. Remaining gaps: the terminal pane still lacks a GPU-mode equivalent and the input/audio plumbing only covers Doom today.
- Layout compiler emits flat nodes only; no nested/z-order metadata.
- Runtime service/bus + multi-transport API still missing.
- CI/visual testing harness still missing.
