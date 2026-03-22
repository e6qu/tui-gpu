## Alacritty Terminal Integration Notes

- `alacritty_terminal::term::Term` owns the screen grid, scrollback, modes, and damage tracking.
- `alacritty_terminal::tty::unix::new` spawns the PTY and child shell, returning a `Pty` (master File + Child process + signal pipe).
- Term updates propagate via an `EventProxy` trait (Alacritty implements this to schedule redraws). We'll need a lightweight proxy (e.g., channel) to notify our renderer when the grid changes.
- PTY output is fed into `Term` via the `alacritty_terminal::event::Processor`, which dispatches parsed events to the term. The processor also handles keyboard/mouse input conversions.
- Grid access happens via `term.grid().visible_lines()` or `term.renderable_content()`, which we can walk to produce glyph quads.
- For inspiration, check `alacritty/src/event.rs` for how they drive the event loop, PTY reader, and term.

### Key APIs for Integration
- `Term::renderable_content()` exposes an iterator over visible lines/cells for rendering.
- `Term::grid()` gives raw access to the active grid if custom traversal is needed.
- `event::Notify` and `Notifier`: implement this to forward OSC/clipboard requests and PTY writes.
- `event::EventListener` (`EventProxy`): implement to receive terminal events (Wakeup, Bell, etc.) and trigger renderer redraws.
- `event_loop::State` uses `vte::ansi::Processor` to parse PTY bytes and apply to `Term`; we can adapt this to feed our `Term` without adopting the full event loop.
