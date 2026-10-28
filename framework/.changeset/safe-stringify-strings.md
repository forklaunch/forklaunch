---
'@forklaunch/common': minor
'@forklaunch/core': patch
'@forklaunch/express': patch
'@forklaunch/hyper-express': patch
'@forklaunch/universal-sdk': patch
---

`safeStringify` now JSON-encodes strings (`safeStringify('123')` is `'"123"'`)
instead of returning them as-is, so `safeParse(safeStringify(s))` gives back the
same string. Before, a string that looked like a number, boolean or JSON came
back as that type; Redis cache values were affected.

New `toPlainString` for headers, form fields and query values, where a string
should stay unquoted. The express and hyper-express `setHeader` wrappers, the
universal SDK and the MCP generator use it, so header, form and query values
are unchanged on the wire. Mixed-type enum columns keep storing strings
unquoted, so existing rows still match queries.

If you call `safeStringify` on a string and expect the raw string back, switch
to `toPlainString`.
