---
"kno": patch
---

Fix `kno sync push` for projects that use a configured Knots store root, so
local event files are found and published from the active store instead of only
from `.knots/` under the repository root.

Clarify sync push progress output so a no-op push reports that local files are
being checked, and only reports copied files when files actually need to be
copied into the publish worktree.
