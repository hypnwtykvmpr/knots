---
"kno": patch
---

Preserve user-supplied tag casing on `kno new` and `kno update --add-tag` so a
tag like `Journey-Github-Connect` is stored and shown exactly as entered.
Tag filtering (`kno ls -g`) and add/remove dedup are now case-insensitive, so
existing lowercased tags in established projects remain queryable and the
first-seen spelling wins when a tag is re-added with a different case.
