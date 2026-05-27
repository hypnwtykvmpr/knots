---
"knots": patch
---

Fix execution plan cache replay loss so plan-step edits (including removals) are preserved correctly when the local cache rehydrates from the event log.
