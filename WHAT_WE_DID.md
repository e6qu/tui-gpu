## WHAT WE DID
- Authored architecture/research docs and published repo (AGPLv3).
- Implemented renderer skeleton + layout compiler CLI.
- Added runtime-core (event log + CAS) and runtime CLI.
- Wired renderer to load compiled layouts and use Taffy for layout-driven quads.
- Built initial terminal session crate with PTY spawn/capture; decided to switch to `alacritty_terminal` for terminal buffer handling instead of libvterm.
