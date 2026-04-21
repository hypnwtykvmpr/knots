---
"knots": patch
---

Fix `kno sync` failing on legacy `idx.knot_head` events written before
`workflow_id` became a required field. When `workflow_id` is missing, sync now
infers it from the event's knot type (defaulting to `work_sdlc` for `work`
knots), matching the existing `parse_knot_type` fallback convention. A
one-shot warning reports the inference.
