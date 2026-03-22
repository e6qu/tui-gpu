## WHAT WE DID
- Authored architecture/research docs and published repo (AGPLv3).
- Implemented renderer skeleton + layout compiler CLI.
- Added runtime-core (event log + CAS) and runtime CLI.
- Wired renderer to load compiled layouts and use Taffy for layout-driven quads.
- Built terminal session crate that spawns PTYs, and now maintains a VT buffer via the `vte` parser (with tests).
