---
"knots": patch
---

Knots commands invoked from a Git linked worktree now share the canonical
repo `.knots` store with the primary worktree. Previously the post-merge
sync hook tried to check out the `knots` branch into a per-worktree
`.knots/_worktree` and failed with `fatal: 'knots' is already used by
worktree at '<primary>/.knots/_worktree'`. The store path is now resolved
via `git rev-parse --git-common-dir` so all linked worktrees of the same
repo see one Knots store.
