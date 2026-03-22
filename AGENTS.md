## AGENTS
- Agent sessions have UUIDs and support forking/rebasing histories (per design).
- Terminal session infrastructure now spawns PTYs and maintains a VT buffer via `vte`, enabling agent-driven shells with screen capture.
- Rigorous testing: unit tests plus eventual manual screenshot comparisons.
