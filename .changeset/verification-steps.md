---
"knots": minor
---

Add first-class verification steps to knots. Use `kno new --verification-step "<step>"` (repeatable) when creating a knot, and manage them on existing knots with `kno update --add-verification-step`, `--remove-verification-step`, and `--clear-verification-steps`. The steps are surfaced in `kno show` and in `kno show --json` under `verification_steps`.
