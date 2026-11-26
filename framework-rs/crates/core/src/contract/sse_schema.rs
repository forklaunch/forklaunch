//! Explicit projection from SSE data payload contracts to SDK event envelopes.
use crate::contract::SchemaRef;

/// Native SSE contracts mark the schema of each JSON `data` payload. SDKs expose
/// an envelope containing that payload and the optional SSE `id`. Unmarked
/// schemas retain the legacy envelope convention; field names never infer it.
#[must_use]
pub fn sse_event_schema(schema: &SchemaRef) -> SchemaRef {
    let Some(raw) = &schema.json_schema else {
        return schema.clone();
    };
    let Ok(mut payload) = serde_json::from_str::<serde_json::Value>(raw) else {
        return schema.clone();
    };
    if payload
        .get("x-forklaunch-sse-framing")
        .and_then(serde_json::Value::as_str)
        != Some("payload")
    {
        return schema.clone();
    }
    if let Some(object) = payload.as_object_mut() {
        object.remove("x-forklaunch-sse-framing");
    }
    let relocated = crate::contract::relocate_schema_json_unscoped(
        &payload.to_string(),
        &["properties", "data"],
    );
    let payload: serde_json::Value =
        serde_json::from_str(&relocated).unwrap_or(serde_json::Value::Bool(false));
    SchemaRef::named(format!("{}SseEvent", schema.name)).with_json_schema(
        serde_json::json!({
            "type":"object", "required":["data"], "additionalProperties":false,
            "properties":{"data":payload,"id":{"type":"string"}}
        })
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_payload_projection_preserves_legacy_envelopes_and_local_refs() {
        let source = SchemaRef::named("Payload").with_json_schema(r##"{"x-forklaunch-sse-framing":"payload","$ref":"#/$defs/P","$defs":{"P":{"type":"object","properties":{"data":{"type":"string"}}}}}"##);
        let event = sse_event_schema(&source);
        assert_eq!(event.name, "PayloadSseEvent");
        let value: serde_json::Value =
            serde_json::from_str(event.json_schema.as_ref().unwrap()).unwrap();
        assert_eq!(
            value["properties"]["data"]["$ref"],
            "#/properties/data/$defs/P"
        );
        assert_eq!(sse_event_schema(&event), event);
        let legacy = SchemaRef::named("Legacy")
            .with_json_schema(r#"{"type":"object","properties":{"data":{"type":"string"}}}"#);
        assert_eq!(sse_event_schema(&legacy), legacy);
    }
}

#[cfg(all(test, feature = "typed-schema", feature = "schema-validation"))]
mod typed_tests {
    #[derive(schemars::JsonSchema)]
    #[allow(dead_code)]
    struct Payload {
        data: String,
    }

    #[test]
    fn native_payload_with_a_data_field_is_not_mistaken_for_an_envelope() {
        let body = crate::contract::BodyContract::typed::<Payload>(
            crate::contract::ContentType::EventStream,
        );
        let schema = super::sse_event_schema(&body.schema);
        let value = serde_json::from_str(schema.json_schema.as_ref().unwrap()).unwrap();
        let validate = crate::contract::schema_validation::compile_checked(&value).unwrap();
        assert!(validate(&serde_json::json!({"data":{"data":"payload"},"id":"revision"})).is_ok());
        assert!(validate(&serde_json::json!({"data":"payload"})).is_err());
    }
}
