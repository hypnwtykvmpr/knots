---
"knots": patch
---

Fix `kno rehydrate` and `kno doctor --fix`'s cold-tier sweep silently
failing for knots pulled from origin. `rehydrate_from_events` only
scanned the local `.knots/events/` and `.knots/index/` directories, but
events arriving via `kno sync` pull land in `.knots/_worktree/.knots/`
and are never copied into the local store. Rehydrating such a knot
failed with `missing workflow_id`, and `fix_cold_tier_imbalance`
swallowed the per-knot error — so the doctor warning persisted with
zero rehydrations even when there were plenty of cold candidates.

`rehydrate_from_events` now accepts multiple store roots and callers
pass both the local and worktree roots, deduped by relative filename
so locally-mirrored events aren't replayed twice.
