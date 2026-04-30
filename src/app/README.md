# app

Core business logic for knot operations.

## Key Files

- **`knot_create.rs`** — `create_knot_with_options()`: new knot creation
- **`knot_update.rs`** — `update_knot_with_options()`: field updates (title, body, tags, etc.)
- **`knot_lease.rs`** — lease lifecycle hooks for knot writes
- **`knot_profile.rs`** — profile assignment and migration on knots
- **`state_ops.rs`** — `set_state()`, `write_state_change_locked()`: workflow state transitions
- **`state_resolve.rs`** — cascade/auto-resolve logic for parent/child state changes
- **`gate.rs`** — `evaluate_gate()`: gate review decisions and failure routing
- **`gate_metadata.rs`** — `append_gate_failure_metadata_locked()`: gate failure tracking
- **`edges.rs`** — `apply_edge_change()`: parent/child and dependency edges
- **`execution_plan_edit.rs`** — wave/step add, move, remove for execution plans
- **`archival.rs`** — terminal-state sweep and cold-tier archival
- **`query.rs`** — `list_knots()`: read operations
- **`rehydrate.rs`** — `rehydrate_from_events()`: rebuild state from event log
- **`sync_ops.rs`** — push/pull/sync entry points called by the CLI
- **`helpers.rs`**, **`alias.rs`**, **`profile_config.rs`** — shared utilities
- **`types.rs`** — `KnotView`, `EdgeView`, `ChildSummary`, `CreateKnotOptions`
- **`error.rs`** — `AppError` enum for all app-layer failures

## Key Types

- `App` — main facade; holds SQLite connection, EventWriter, ProfileRegistry
- `KnotView` — full knot representation returned by all operations
- `AppError` — error enum for all app-layer failures
