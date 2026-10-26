# Universal SDK runtime

This project corresponds to TypeScript `framework/universal-sdk`. Core owns contract compilation; generated Rust clients own DTOs, operation signatures, response status variants, and their retained schema bindings. Those clients delegate transport and checked decoding to this shared runtime.

The runtime provides secure HTTP admission, explicit auth and request hooks, bounded SSE and multipart decoding, response schema/header checks, and redacted transport diagnostics. Default clients refuse redirects; custom HTTP clients must retain TLS verification and redirect policy. Request hooks run before final URL admission. Nonmultipart requests remain cloneable even when a different client uses multipart.

Dynamic registry refresh and a configurable content-parser map are not implemented here yet. This crate does not invent application authorization or data-access policy.

Versioned generated clients retain the schemas of every declared branch, but admit responses only from branches matching the actual request after auth and request hooks. Body and complete header alternatives are checked independently. The static response type conservatively includes all branches; it never authorizes an unmatched response. SSE applies the same rule per event and closes before exposing an invalid frame.

Portable matching currently covers path and query fields, explicitly supplied `x-*`/authorization/content-type headers, and finite JSON, text, binary, and nested URL form bodies with explicit portable parser settings. Captured path segments decode exactly once. JSON is strict, BOM-aware, and bounded; URL forms share the native bounded parser and generated TypeScript uses canonical `qs`. Unsupported parser settings, multipart responses, and implicit browser-managed header predicates fail generation explicitly. Multipart request matching uses the restricted profile below. Runtime-only application preprocessors are outside the portable schema contract. Native versioned SSE handlers use the same schema selection, authorization lifecycle, and per-frame matched-branch validation.

Versioned multipart requests use a bounded **portable multipart profile** for
client-side version selection: at most 128 unique ASCII field names, 1 MiB per
field, 2 MiB total wire bytes, strict UTF-8 text, and nonempty ASCII filenames for
binary parts. Both clients validate the exact serialized bytes they send. Rust
streaming request bodies must finish buffering within 30 seconds; parser errors,
limits, duplicate names, and unsupported headers/charsets deny the request before
network dispatch. These restrictions apply to version matching, not ordinary
multipart uploads.

This is not arbitrary Busboy compatibility. The canonical TypeScript middleware
commits file fields on stream completion, so mixed file/text duplicate names can
produce different values from a sequential parser; its default parameter charset
also differs for non-ASCII names. Such inputs are rejected by this portable
profile. Existing native multipart endpoints and ordinary Notes attachment
uploads retain their existing policies.

Portable parser metadata supports JSON `strict`, per-parser byte limits up to
2 MiB, and URL form `extended`, `depth` (up to 32), `parameterLimit` (up to
100,000), `charsetSentinel`, and `interpretNumericEntities`. Partial settings keep
native defaults; unknown settings and invalid types fail generation. Wire
`inflate` can be explicitly recorded, but SDK matching still rejects compressed
request bodies rather than pretending to apply decompression. Defaults must be
UTF-8; alternate default charsets and runtime-only verifier/reviver functions are
outside this portable profile. Charset sentinel processing of percent-encoded
form data uses the same native parser and canonical `qs` behavior.

## Service-to-service HMAC

`with_hmac_secret(key)` (or `with_hmac_signer(HmacSigner::with_key_id(id, key))`)
signs every request with the ForkLaunch request HMAC, after defaults and request
hooks, over the final method, URL path without the query, and the exact bytes sent
(streaming multipart bodies are buffered first). Each request gets a fresh timestamp
and a random nonce, so verifiers that claim nonces reject replays. The signed path is
the full wire path (`POST /payment-link`), the same as every Rust verifier and the
TypeScript SDK signer. `with_hmac_authorization` sends one fixed value, which an HMAC
verifier accepts at most once.
