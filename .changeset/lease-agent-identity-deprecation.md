---
"knots": patch
---

Agent identity is now stamped from the bound lease across every write sink
(next, rollback, claim, poll-claim, gate evaluate, step annotate, state, note,
handoff). The per-command `--agent-name`, `--agent-model`, `--agent-version`,
`--note-agent-*`, and `--handoff-agent-*` flags are still accepted for
compatibility but are runtime-ignored and emit a deprecation warning on stderr;
their help text is prefixed `[DEPRECATED — IGNORED]`. The deprecation warning
uses singular or plural wording depending on how many flags were supplied. `kno
lease create` is unchanged — the lease remains the single declared source of
agent identity.
