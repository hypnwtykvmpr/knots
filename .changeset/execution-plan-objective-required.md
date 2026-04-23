---
"knots": patch
---

Execution plan knots now require an objective at creation and on update. `kno
new` and `kno update` reject execution-plan knots that lack an objective, and
the execution-plan docs describe the field. Existing non-execution-plan knots
are unaffected.
