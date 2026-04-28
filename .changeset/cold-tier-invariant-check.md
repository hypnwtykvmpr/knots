---
"knots": patch
---

`kno doctor` no longer warns forever about `cold_tier_imbalance`. The check
now measures three real tier invariants — disjointness (no id in both hot and
cold), cold-is-terminal-only, and no stale-terminal hot rows — instead of
treating "fewer than 100 hot knots and a non-empty cold catalog" as a problem.
A repo whose cold tier holds only legitimately-old shipped/abandoned knots is
healthy. `kno doctor --fix` restores each invariant idempotently and the next
sync, sweep, and snapshot bootstrap all uphold them, so doctor stays `pass`.
See `docs/tier-balance.md`.
