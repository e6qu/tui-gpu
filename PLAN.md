## PLAN
1. Integrate `alacritty_terminal` as our terminal core:
   - Wire PTY output into `alacritty_terminal`’s grid, expose snapshots, unit tests.
2. Render terminal buffer in renderer (glyph cache + text drawing).
3. Enhance layout compiler (nested nodes/z-order) feeding Taffy tree.
4. Build runtime event bus/API + tests.
5. Add CI + visual smoke tests (tmux/screenshots).
