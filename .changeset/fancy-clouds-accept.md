---
"knots": patch
---

Fix gitignore setup for Knots-managed project directories. `kno init` now
ensures `.knots` is ignored, while `kno skills install` ignores `.agents` and
`.claude` contents except for their allowlisted `skills` directories.
