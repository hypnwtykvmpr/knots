"knots": minor
---

### Features
- add native execution plan knots, persistence, and CLI editing flows
- add orchestration workflow support for execution plans
- add `kno new --tag` for tagging knots at creation time
- add `cold_tier_imbalance` doctor checks and repair paths

### Fixes
- migrate workflow state handling away from the legacy `KnotState` enum
- accept legacy execution plan `beat_ids` during compatibility transitions
- tighten cold-tier archival behavior and coverage around execution plan flows
