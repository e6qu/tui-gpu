## Alacritty Terminal Integration Notes

- `alacritty_terminal::term::Term` owns the screen grid, scrollback, modes, and damage tracking.
- `alacritty_terminal::tty::unix::new` spawns the PTY and child shell, returning a `Pty` (master File + Child process + signal pipe).
- Term updates propagate via an `EventProxy` trait (Alacritty implements this to schedule redraws). We'll need a lightweight proxy (e.g., channel) to notify our renderer when the grid changes.
- PTY output is fed into `Term` via the `alacritty_terminal::event::Processor`, which dispatches parsed events to the term. The processor also handles keyboard/mouse input conversions.
- Grid access happens via `term.grid().visible_lines()` or `term.renderable_content()`, which we can walk to produce glyph quads.
- For inspiration, check `alacritty/src/event.rs` for how they drive the event loop, PTY reader, and term.
