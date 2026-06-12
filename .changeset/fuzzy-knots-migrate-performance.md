---
"knots": patch
---

Repair `kno sync-ref migrate` for large Knots refs by batching remote blob reads, avoiding full repository clones, skipping unchanged publishes, and filtering migration inputs to canonical Knots JSON store paths.
