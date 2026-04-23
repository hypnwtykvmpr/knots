---
"knots": patch
---

Gate evaluation prompts now carry the knot's acceptance criteria, so the
evaluator sees the same contract the author wrote. Prompt resolution threads
acceptance through `gate_sdlc/prompts/evaluating.md` and the knots prompt
pipeline; no flag changes, but evaluations gain context automatically on the
next claim.
