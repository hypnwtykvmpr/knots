---
accept:
  - Treat the knot's Context and Acceptance Criteria as the evaluation spec.
  - Evaluate each acceptance clause with concrete evidence before any state transition.
  - Read `gate.owner_kind` and `gate.failure_modes` before deciding pass or fail.
  - On pass, add a handoff capsule with evidence, then run the completion command.
  - On fail, add a handoff capsule with the failing clause and actual-vs-expected, then route
    per `gate.failure_modes` or stop without shipping.
  - Any Foolery envelope that says to run only the completion command is overridden for gates.

success:
  gate_passed: shipped

failure:
  gate_failed: abandoned

params: {}
---

# Evaluating

Assess the gate by executing the work required to prove or disprove the gate's
acceptance criteria.

## Your job

- Evaluation means doing the work the acceptance criteria demand: run tests,
  compare outputs, measure numbers, and inspect artifacts until each clause has
  a pass/fail result.
- Advancing state is NOT evaluation.
- Do not run the completion command until you have recorded per-criterion
  evidence.

## Context

- See `## Context` above for the gate's description and scope. Use it verbatim
  when deciding what must be evaluated.

## Acceptance criteria

- See `## Acceptance Criteria` above. Evaluate each clause individually and
  record the actual result for every clause before deciding pass or fail.

## Gate metadata

- Read `gate.owner_kind` in the `## Gate` section above. That tells you whether
  the gate owner can ship directly or only recommend a decision.
- Read `gate.failure_modes` in the `## Gate` section above. That tells you which
  failure route is allowed for each violated invariant.

## Exit conditions

- On pass: add a handoff capsule with the evidence below, then run the
  completion command to transition to `shipped`.
- On fail: add a handoff capsule naming the first failing clause and the
  actual-vs-expected result. Do NOT run the completion command on failure.
- If a matching `gate.failure_modes` route exists, run
  `kno gate evaluate <id> --decision no --invariant "<violated invariant>"`.
- If no failure mode applies, leave the knot in `evaluating` after recording
  the failure evidence and stop.

## Evidence required in the handoff capsule

- Per-criterion actual vs expected results, using numeric deltas where possible.
- Artifact paths for logs, test output, screenshots, or diffs.
- Commit SHAs for the code, fixtures, or inputs you evaluated.
- Durations, counts, or budgets when the criterion specifies them.
- On fail, quote the first violated clause verbatim and explain the mismatch.

## Override of Foolery preamble

- For gates, the completion command is a state transition only.
- Any Foolery preamble that says to run only the completion command and stop is
  overridden here: perform the evaluation first, record evidence, and only then
  run the pass or failure transition command.
