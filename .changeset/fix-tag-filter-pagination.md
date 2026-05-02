---
"knots": patch
---

Tag, query, profile, and `--all` filters now apply before pagination on
`kno ls --json`. Previously, only `--state` and `--type` were pushed into
SQL, while tag/query filters ran after `LIMIT`/`OFFSET` was applied. As a
result, `kno ls --tag <tag> --json --limit 1` could return an empty data
array even when matches existed, and `total` reported the unfiltered hot
count instead of the filtered match count. The paginated path now
materializes the full hot tier, applies the same filter pipeline as the
non-paginated path, sets `total` to the filtered count, and slices the
filtered list by `offset`/`limit`. Pages are stable across offsets and
`has_more` is consistent with `total` and `data.length`.
