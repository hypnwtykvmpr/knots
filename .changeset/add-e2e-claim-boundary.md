---
"knots": patch
---

Add `--e2e` support to `kno claim` and `kno poll` so end-to-end agent runs can
receive an explicit `e2e_continuation` workflow boundary. JSON claim output now
includes `e2e` and `workflow_boundary_kind`, while ordinary claims continue to
emit the default `single_action` boundary.
