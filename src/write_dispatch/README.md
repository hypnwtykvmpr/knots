# write_dispatch

Maps CLI write commands to operations and executes them through the write queue.

## Key Files

- **`../write_dispatch.rs`** — module root: `maybe_run_queued_command_with_context()`
- **`operation_map.rs`** — `operation_from_command()`: CLI args to `WriteOperation`
- **`execute/mod.rs`** — `execute_operation()`: dispatches operations to App methods
- **`execute/execute_write_ops.rs`** — individual write operation handlers
- **`execute/execute_plan_ops.rs`** — execution-plan wave/step mutation handlers
- **`helpers.rs`** — shared formatting and output helpers

## Flow

```
CLI args -> operation_from_command() -> write_queue -> execute_operation() -> App methods
```

All writes serialize through the FIFO queue to prevent concurrent modification.
