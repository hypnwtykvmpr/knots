---
"knots": patch
---

`kno doctor --fix` now emits per-operation progress lines so long-running
repairs no longer look like a silent hang. The command announces the
diagnostic phase, prints a `Fixing <check>...` line as each fix starts, and
prints a `N issue(s) fixed.` summary on completion. Running `kno doctor`
without `--fix` and `kno doctor --fix --json` are unchanged.
