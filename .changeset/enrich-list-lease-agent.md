---
"kno": patch
---

Enrich `kno ls --json` output for active action knots with bound lease agent
metadata. The `lease_agent` object now mirrors `kno show --json` by including
provider, agent, model, and version details when a lease is bound, while queued
knots without a bound lease continue to omit `lease_agent`.
