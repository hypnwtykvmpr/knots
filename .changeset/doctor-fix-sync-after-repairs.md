---
"knots": patch
---

Fix `kno doctor --fix` leaving `workflow_id_parity` (and other event-log
repairs) perpetually warning after a fix run. Repair events are emitted
into the local `.knots/index/` store, but the check scans the shared
`_worktree`, so the post-fix recheck never saw them. `apply_fixes` now
reports whether the event log was touched, and `run_doctor_with_fix_at`
publishes the repair events via a best-effort sync before the recheck.
Also prevents each subsequent `kno doctor --fix` from stacking duplicate
repair events for the same stale knots while waiting for a sync.
