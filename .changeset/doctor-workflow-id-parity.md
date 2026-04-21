---
"knots": patch
---

Add a `workflow_id_parity` doctor check and `--fix` path. The check scans the
pulled worktree for knots whose latest `idx.knot_head` event lacks
`workflow_id`. `kno doctor --fix` publishes a minimal repair event per stale
knot so the shared event log eventually reaches parity with modern events —
active knots use the local DB state, archived knots use the cold catalog plus
the workflow inferred from the stale event's knot type.
