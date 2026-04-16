# events

Event file I/O. Writes JSON event files to `.knots/events/` and `.knots/index/`.

## Key Files

- **`mod.rs`** — `EventWriter::write()`, `relative_path_for_event()`, `new_event_id()`
- **`error.rs`** — `EventWriteError` for I/O and serialization failures

## Key Types

- `EventRecord` — wraps either a `FullEvent` or `IndexEvent`
- `FullEvent` / `FullEventKind` — complete knot change events
- `IndexEvent` / `IndexEventKind` — lightweight head summaries used for fast sync
- `WorkflowPrecondition` — ETag guard for optimistic writes

## Event Layout

- Full events: `.knots/events/YYYY/MM/DD/<uuid>-<type>.json`
- Index events: `.knots/index/YYYY/MM/DD/<uuid>-idx.knot_head.json`

Index events are lightweight summaries enabling fast sync without full event transfer.
