---
name: knots-e2e
description: >-
  Use the Knots workflow through `kno` when asked to drive a knot end to end,
  run a claimed knot to completion, or keep advancing a knot until it reaches a
  terminal state such as `SHIPPED`, or a passive waiting state such as
  `BLOCKED` or `DEFERRED`.
---

# Knots E2E

## Invocation precedence

When this skill is invoked, the agent MUST claim with the `--e2e` flag so the
workflow boundary surfaced by `kno` advertises e2e-continuation semantics
rather than the default single-action boundary:

```bash
kno claim --e2e <id>
```

The claim output will then carry `kind: e2e_continuation` in the workflow
boundary section (and `"workflow_boundary_kind": "e2e_continuation"` plus
`"e2e": true` in `--json` output). That signal authorizes the agent to keep
working past the per-claim "complete exactly one workflow action" boundary
that ordinary claims emit. After every successful `kno next`, immediately
re-claim with `kno claim --e2e <id>` and continue until the knot reaches
`SHIPPED`, `BLOCKED`, or `DEFERRED`.

If the claim output shows `kind: single_action` (i.e. `--e2e` was not
passed), do NOT continue past the boundary. Stop after the single action
and ask the user to re-invoke with the e2e skill if they want to continue.

## Workflow

Follow this sequence:

```bash
kno claim --e2e <id>
```

- If you are working inside a git worktree, run Knots commands as
  `kno -C <path_to_repo> ...` because Knots is installed for the repo root,
  not the worktree path.
- Record the current state from the claim output.
- If the current state is `SHIPPED`, `BLOCKED`, or `DEFERRED`, stop cleanly.
- Use the claim output to determine the current state's completion goals.
- Do the work and validate it.
- If the goals were met, advance with a guarded state check:

```bash
kno next <id> --expected-state <current_state>
```

- Record the new current state from the `kno next` output.
- Re-claim immediately, again with `--e2e`, to enter the next action state:

```bash
kno claim --e2e <id>
```

- Repeat the claim/work/validate/advance loop until the current state is
  `SHIPPED`, `BLOCKED`, or `DEFERRED`.
- If you are blocked, validation fails, or the state's goals were not met,
  roll back safely and stop:

```bash
kno rollback <id>
```

If the claimed knot lists children, handle the children first:
- Claim each child knot (with `--e2e`) and follow that child prompt to
  completion.
- When the child knots are handled, evaluate the outcomes.
- If every child advanced, advance the parent and continue the loop.
- If any child rolled back, roll the parent back and stop.

Do not invent alternate transition workflows. Prefer `claim`, `next`, and
`rollback` over manual state mutation unless the user explicitly asks for it.
Do not use `kno show` as the primary control-flow source when `claim`/`next`
already provide the state needed to continue safely.

## Why `--e2e` exists

Ordinary `kno claim <id>` emits a workflow boundary instructing the agent to
"complete exactly one workflow action, then stop." That boundary is correct
for one-shot claims and prevents agents from over-running their authorization.
When the user has explicitly asked for end-to-end execution, however, the
default boundary fights the skill. Passing `--e2e` switches the boundary to
`e2e_continuation`, which:

1. Carries an unambiguous, machine-readable signal (`workflow_boundary_kind`)
   so agents can detect e2e mode without inferring intent from prose.
2. Tells the agent to re-claim after `kno next` and continue across action
   states.
3. Authorizes terminal-state movement for this run, since `SHIPPED` /
   `BLOCKED` / `DEFERRED` are valid stopping points for an e2e run.

Ordinary one-action claims continue to use the `single_action` boundary;
`--e2e` is opt-in.

## Session close behavior

- In an interactive session, briefly say what changed and the final knot state.
- In a non-interactive session, stop cleanly after the knot workflow is complete.
