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

Two outputs do change for strings:
- A string passed as `body.json` to the universal SDK is now sent as a JSON
  string (quoted). If you were passing pre-serialized JSON text, pass the
  object instead, or it is encoded twice.
- An MCP tool whose JSON response is a top-level string now shows it quoted,
  as the Rust MCP server already did.

Cached string values written before this change were stored unquoted. They
still read back, but one that looks like JSON (`'123'`) reads as that type
until it is written again. The Rust `JsonRecordCodec` in TypeScript-compatible
mode now also writes strings as JSON, so both sides agree.

If you call `safeStringify` on a string and expect the raw string back, switch
to `toPlainString`.
