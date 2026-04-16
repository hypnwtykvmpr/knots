# sync

Git-based event replication between local and remote. `push` + `pull` live in the sibling `replication.rs` module; this module owns the pull/apply mechanics.

## Key Files

- **`mod.rs`** — `SyncService::sync()` / `sync_with_progress()`, `SyncSummary`, `SyncError`
- **`apply.rs`** — `IncrementalApplier`: applies index and full events to SQLite cache
- **`apply_helpers.rs`** — helper functions for event application
- **`git.rs`** — `GitAdapter`: fetch, reset, commit, push primitives
- **`worktree.rs`** — `KnotsWorktree`: manages the `.knots/_worktree` git worktree

## Key Types

- `SyncService` — coordinates a pull pass
- `SyncSummary` — counts of applied index/full events, drift info
- `GitAdapter`, `KnotsWorktree` — re-exported from submodules

## Data Flow

```
push: scan local events -> copy to worktree -> git commit -> git push
pull: git fetch -> reset worktree -> apply index events -> apply full events -> update cache
```
