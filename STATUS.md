## STATUS
- Renderer: Taffy layout integration renders panes/menus/overlays; ready to accept terminal buffer content.
- Terminal session crate spawns PTY-backed shells and captures output; next step is replacing libvterm plan with `alacritty_terminal` for buffer handling.
- Layout compiler + runtime core/CLI are committed; docs/specs synced.
