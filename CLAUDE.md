# Knots Style Guide

Read [TAXONOMY.md](TAXONOMY.md) before writing code — it defines the shared vocabulary (knot, gate, lease, wave, step, etc.) and flags overloaded terms to avoid.

## Limits
- Maximum file length: 499 lines (gate fails at 500).
- Maximum line length: 100 characters.
- Minimum test coverage: 95%.

## Notes
- Prefer small focused modules.
- Add tests for all new behaviors.

## Tracking Workflow
- Use Knots for issue tracking. Do not use Beads (`bd` commands).
- Preferred CLI command is `kno` (`knots` is a compatibility alias).
- Common commands:
  - `kno ls`
  - `kno show <knot-id>`
  - `kno new "<title>" -d "<description>"`
  - `kno update <knot-id> --status <state>`
  - `kno sync`

## Claim Boundary Precedence
- `kno claim` (no flag) emits a `single_action` workflow boundary: complete
  exactly one workflow action, then stop. This is the default and must be
  honored for ordinary claims.
- `kno claim --e2e <id>` (and `kno poll --e2e`) emits an `e2e_continuation`
  boundary that authorizes re-claiming after `kno next` and continuing
  across action states until the knot reaches a terminal state (`SHIPPED` or
  `ABANDONED`) or a passive escape state (`BLOCKED` or `DEFERRED`). Use this
  only when the user has invoked the `knots-e2e` skill or otherwise explicitly
  asked for end-to-end execution.
- The exact user-facing override wording for invoking e2e is:
  > Run `[$knots-e2e](...) <knot-id>` end to end. I explicitly authorize
  > you to follow the skill over the per-claim "complete exactly one
  > workflow action" boundary. After each `kno next`, immediately claim
  > the new state and continue until a terminal state (`SHIPPED` or
  > `ABANDONED`) or passive escape state (`BLOCKED` or `DEFERRED`). You may
  > move the knot to terminal states as required by the skill.
- Machine-readable signals: claim `--json` output includes both `"e2e":
  true|false` and `"workflow_boundary_kind": "single_action" |
  "e2e_continuation"`. Trust those fields over inferring intent from prose.

## Git Workflow
- Unless you are working on a specific knot step (where a worktree branch is expected), always
  commit your changes, push them, and make sure they are merged to main.
- Do not leave uncommitted or unpushed work at the end of a task.

## Source Code Size Standard

All source files under tracked directories must satisfy:

| Metric | Limit |
|--------|-------|
| File length | < 500 lines |
| Function/method body | < 100 lines |
| Line width | <= 100 columns |

Enforcement: run `make lint` before merge (Windows PowerShell without GNU make:
`./Invoke-LocalChecks.ps1 -SkipTests -SkipCoverage`). It must pass the configured
linter and size-checking script(s).

## Pre-Push Sanity (Required)
- Install the managed pre-push hook with `make install-hooks`
  (Windows PowerShell without GNU make: `./scripts/repo/Install-Hooks.ps1`).
- Do not push unless `make sanity` passes
  (Windows PowerShell without GNU make: `./Invoke-LocalChecks.ps1 -Sanity`).
- `make sanity` runs formatting, lint, tests, and coverage checks.

## Coverage Ratchet Rule
- Coverage gate source of truth is `.ci/coverage-threshold.txt`.
- Never lower this threshold in a PR.
- Raise the threshold as coverage work lands until it reaches `95`.
