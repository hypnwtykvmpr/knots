---
"kno": patch
---

Clarify the `planning` workflow prompt so agents create implementation child
knots without blocking the parent's `plan_review` transition. The prompt now
explicitly recommends `kno q "<title>"`, `kno new "<title>" -f`, or
`kno new "<title>" -p autopilot_no_planning` for child knots created during
planning, documents the create-then-link order with
`kno edge add <parent> parent_of <child>`, and adds a Hierarchy Gate note
explaining why a child created with the default `autopilot` profile (rank 1)
blocks the parent's `plan_review` transition (rank 3). Hierarchy gate
semantics are unchanged.
