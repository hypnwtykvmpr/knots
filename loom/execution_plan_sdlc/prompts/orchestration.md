---
accept:
  - The execution plan has been driven forward according to its next queued work
  - The orchestration outcome is recorded in a handoff-ready note
  - Downstream progress or blockers are clear from the orchestration output

success:
  orchestration_complete: shipped

failure:
  blocked_by_dependency: blocked

params: {}
---

# Orchestration

Drive the approved execution plan forward after review has passed.

## Step Boundary

- This session is authorized only for `orchestration`.
- Complete exactly one orchestration action, then stop.
- Allowed resting states after this session: `shipped` or `blocked`.
- Record the orchestration outcome in the note output before advancing.

## Actions

1. Review the approved execution plan and the current status of its waves and
   steps.
2. Advance the next ready work in the plan according to the documented
   sequencing constraints.
3. Record the orchestration outcome, including any blockers or handoffs needed
   for downstream follow-through.
4. Advance to `shipped` when orchestration is complete.
