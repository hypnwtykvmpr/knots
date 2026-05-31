---
"knots": patch
---

Fix synced terminal knots so they stay in the hot tier during the 72-hour grace
window, keeping recently shipped or abandoned work list-visible. `kno doctor --fix`
now also repairs recent terminal rows that were incorrectly placed in the cold
catalog.
