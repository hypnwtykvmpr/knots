---
"knots": patch
---

Skip imported knot heads referencing unknown workflows during sync instead of failing. A warning is emitted with the knot id and workflow id so the workflow can be installed in the repository, and follow-on full events for skipped knots are ignored.
