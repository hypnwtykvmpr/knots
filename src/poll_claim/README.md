# poll_claim

Find and claim the highest-priority ready knot for the current agent.

## Key Files

- **`../poll_claim.rs`** — module root: `run_poll()`, `run_claim()`, `peek_knot()`, `claim_knot()`, `PollResult`
- **`ready.rs`** — `run_ready()`, `list_queue_candidates()`: queue filtering

## Behavior

- `kno ready`: inspect queue inventory, optionally filter by stage/owner
- `kno poll`: peek at next claimable knot
- `kno poll --claim`: claim and return action prompt
- `kno claim`: claim a specific knot by ID
- `ready` shows the upcoming owner/action for each queued knot
- `poll` and `claim` respect profile ownership (human vs agent) and lease state
