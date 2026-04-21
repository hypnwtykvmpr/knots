---
"knots": patch
---

Fix `kno sync` hard-failing on legacy events that predate the
2026-04-09 strictness change ("Remove legacy workflow runtime
fallbacks"). Two tolerant fallbacks are restored at apply time so a
bootstrap pull of a pre-cutoff repo no longer errors out with
`missing 'profile_id' string field`:

- `required_profile_id` defaults to `"autopilot"` when the field is
  missing or empty, instead of erroring. Silent; the event log stays
  intact and the cache row carries the modern default.
- `required_workflow_id` now recognizes the pre-registry name
  `"default"` as a legacy value alongside `"compatibility"` and
  `"knots_sdlc"`, translating it to `"work_sdlc"` at apply time with
  the same one-time warning pattern.

Event files on disk are not rewritten — this is read-time translation,
consistent with the existing legacy-value handling.
