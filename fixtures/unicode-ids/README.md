# unicode-ids fixture

`ids.json` is a list of string ids: Greek, CJK, emoji, embedded quotes, an
apostrophe, and whitespace. Adapter unit tests return them in this exact order
through `Hit.id` and assert that no escaping, normalisation or re-sorting occurs.
The engine contract carries numeric collection ids but string hit ids, so this
fixture tests the boundary where an adapter could accidentally lose characters.
