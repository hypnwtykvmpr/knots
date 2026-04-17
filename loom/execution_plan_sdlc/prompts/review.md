---
accept:
  - The execution plan design is complete and internally consistent
  - The proposed waves and dependencies are reviewable and actionable
  - The artifact is ready to serve as the source plan for execution

success:
  approved: ready_for_orchestration

failure:
  changes_requested: ready_for_design
  blocked_by_dependency: blocked

params: {}
---

# Review

Review the authored execution plan before it advances to orchestration.

## Step Boundary

- This session is authorized only for `review`.
- Complete exactly one review action, then stop.
- Allowed resting states after this session: `ready_for_orchestration`,
  `ready_for_design`, or `blocked`.
- Review work is metadata-only and should not modify repository code.

## Actions

1. Inspect the authored plan artifact and its supporting knot context.
2. Confirm the design is complete, coherent, and actionable.
3. Approve the plan to advance to orchestration, or send it back for revision.
