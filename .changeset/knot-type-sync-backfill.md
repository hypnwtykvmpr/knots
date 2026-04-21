---
"knots": patch
---

Fix `kno ls --type <type>` silently missing knots first materialized by
a `kno sync` pull. The sync-apply path never read `type` from
`idx.knot_head` event data when upserting a new cache row, so every
knot authored on another machine landed in the local `knot_hot` table
with `knot_type` NULL or empty — `kno ls --type execution_plan` (for
example) filtered them out.

Two changes:
- `build_index_upsert` now reads `data.type` from the index event and
  falls back to the existing cached value only when the event omits
  it. Future pulls populate `knot_type` correctly on first apply.
- New `knot_type_backfill` doctor check + `--fix` path backfills
  already-corrupted caches by scanning the worktree's latest
  `idx.knot_head` event per affected knot and writing the type back to
  `knot_hot`. No event emission — purely a local cache repair.
