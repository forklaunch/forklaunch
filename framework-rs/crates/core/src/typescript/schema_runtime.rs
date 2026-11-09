use std::fmt::Write as _;

/// Shared portable schema validator emitted into HTTP and WebSocket clients.
#[must_use]
pub fn typescript_schema_runtime_source() -> String {
    let mut source = String::from(include_str!("schema-runtime.ts"));
    source.push_str(include_str!("date-codec.ts"));
    source.push_str(&include_str!("zod-checks.ts").replace("import type Ajv from 'ajv';", ""));
    source.push_str(&include_str!("zod-regex.ts").replace("import type Ajv from 'ajv';", ""));
    let schema = serde_json::Value::String(include_str!("draft6-schema.json").into()).to_string();
    let _ = writeln!(
        source,
        "const socketDraft6MetaSchema = JSON.parse({schema}) as Record<string, unknown>;"
    );
    source
}
