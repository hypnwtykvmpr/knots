---
"knots": patch
---

Fix managed skills gitignore rules so `.agents` and `.claude` are recursively
ignored except for their `skills` directories. `kno doctor --fix` now also
repairs legacy managed-skills gitignore rules when skills are already installed.
