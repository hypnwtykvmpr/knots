---
"knots": patch
---

Deprecate `--agent-name`, `--agent-model`, and `--agent-version` on `kno claim`.
The flags still work and continue to stamp metadata on the auto-created lease,
but the canonical pattern is now `kno lease create` followed by `kno claim
--lease <id>`. Using the deprecated flags emits a warning to stderr and they
will be removed in a future release.
