## WHAT WE DID
- Authored architecture/research docs and published repo (AGPLv3).
- Implemented renderer skeleton + layout compiler CLI.
- Added runtime-core (event log + CAS) and runtime CLI.
- Wired renderer to load compiled layouts and use Taffy for layout-driven quads (back online in GUI mode, rendering both layout rectangles and PTY glyphs).
- Built terminal session crate that spawns PTYs and maintains a VT buffer via the `vte` parser (now hooked up again in GUI mode).
