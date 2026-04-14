---
accept:
  - Objective and scope are reflected in the plan design
  - Waves and dependencies are coherent
  - The plan records clear next actions for downstream execution

success:
  design_complete: ready_for_review

failure:
  insufficient_context: ready_for_design
  blocked_by_dependency: blocked

params: {}
---

# Design

Author the execution plan artifact for this knot.

## Step Boundary

- This session is authorized only for `design`.
- Complete exactly one design action, then stop.
- Allowed resting states after this session: `ready_for_review`,
  `ready_for_design`, or `blocked`.
- Record the design in the plan artifact output before advancing.

## Actions

1. Review the knot goal, description, and existing context.
2. Draft a coherent execution plan structure with waves, dependencies, and
   handoff-ready details.
3. Ensure the authored plan is suitable for a later review pass.
4. Advance to `ready_for_review` when the design is ready.
