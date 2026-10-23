# Rust HTTP client generation

`generate_rust_http_sdk_source` generates schema-typed JSON bodies alongside the existing typed status responses. The CLI HTTP SDK generator uses the same model compiler and transport template.

For a JSON operation named `Create`, the generated API includes:

```rust,ignore
// Generated from the operation's JSON Schema.
let body = CreateRequestBody { name: "Ada".into() };
let input = CreateRequest { body: Some(body), ..Default::default() };
let response = client.create(input).await?;
```

The exact fields follow the schema, including nested models, arrays, unions, optional properties, and nullable values. A schema without declared structure uses `serde_json::Value`. `None` omits the body; `Some` serializes the value as JSON, including a JSON string or explicit null. These types do not replace server-side validation of schema constraints.

`create_request` serializes the typed input and returns `Result<SdkRequest, serde_json::Error>`. `create` reports serialization failures as `SdkError::Encode` before running the authentication hook or sending a request. Hooks see the serialized wire body. Request wrapper diagnostics redact body contents.

Encoded JSON is available through the explicit compatibility escape hatch:

```rust,ignore
let input = CreateRequest::<String> {
    body: Some(r#"{"name":"Ada"}"#.into()),
    ..Default::default()
};
let response = client.create_raw(input).await?;
// Or construct the low-level transport with client.create_raw_request(input).
```

This changes generated JSON operation inputs from encoded strings to typed values. Existing consumers should construct the generated body type, or choose `_raw` deliberately when testing malformed wire data or implementing a custom serializer. Text, binary, multipart, and URL-encoded inputs retain their transport-specific APIs. Response types and decoding remain unchanged. The older transport-only `generate_rust_sdk_source` remains a low-level encoded request scaffold.
