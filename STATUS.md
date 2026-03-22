## STATUS
- Renderer: Taffy layout integration renders panes/menus/overlays; ready to accept terminal buffer content.
- Terminal session crate now spawns PTY-backed shells **and** maintains a VT buffer using the `vte` crate (unit tests pass with `printf`).
- Layout compiler + runtime core/CLI + docs are up-to-date.
