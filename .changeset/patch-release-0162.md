---
"knots": patch
---

**Bug fixes**

- Fix workflow id parity doctor repairs to correctly handle workflow ID mismatches
- Fix stale artifact reaper freshness check to properly detect and clean stale local build artifacts
- Fix managed_skills test env_lock race between tests and tests_ext modules

**Improvements**

- Notice managed skill update changes — surface when managed skills are updated
- Reap stale local build artifacts on clean builds
- Document lease-backed skill handoffs in agent skills
