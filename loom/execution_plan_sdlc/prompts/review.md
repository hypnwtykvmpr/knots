---
accept:
  - The execution plan design is complete and internally consistent
  - The proposed waves and dependencies are reviewable and actionable
  - The artifact is ready to serve as the source plan for execution

success:
  approved: shipped

failure:
  changes_requested: ready_for_design
  blocked_by_dependency: blocked

params: {}
---

# Review

Review the authored execution plan before it is considered shipped.

## Step Boundary

- This session is authorized only for `review`.
- Complete exactly one review action, then stop.
- Allowed resting states after this session: `shipped`, `ready_for_design`,
  or `blocked`.
- Review work is metadata-only and should not modify repository code.

## Actions

1. Inspect the authored plan artifact and its supporting knot context.
2. Confirm the design is complete, coherent, and actionable.
3. Approve the plan to move it to `shipped`, or send it back for revision.
