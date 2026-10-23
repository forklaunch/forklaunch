use super::{compile, compile_multipart};
use serde_json::json;

#[test]
fn semantic_uri_literal_data_is_not_a_schema() {
    let literal = json!({"format":"forklaunch-whatwg-url"});
    let schema = json!({"const":literal,"default":literal});
    assert!(compile(&schema).unwrap()(&literal).is_ok());
    let rfc = compile(&json!({"type":"string","format":"uri"})).unwrap();
    assert!(rfc(&json!("https://example.com")).is_ok());
    assert!(rfc(&json!(" https://example.com ")).is_err());
    assert!(compile(&json!({"type":"string","format":"application-custom"})).is_ok());
}

#[test]
fn semantic_uri_nested_and_referenced_schema_positions() {
    for schema in [
        json!({"type":"string","format":"forklaunch-whatwg-url"}),
        json!({"$ref":"#/default/target","default":{"target":{"type":"string","format":"forklaunch-whatwg-url"}}}),
        json!({"allOf":[{"type":"string","format":"forklaunch-whatwg-url"}]}),
    ] {
        for compile_profile in [compile, compile_multipart] {
            let compiled = compile_profile(&schema);
            #[cfg(not(feature = "zod-uri"))]
            assert!(
                compiled
                    .err()
                    .unwrap()
                    .contains("requires the forklaunch-core zod-uri feature")
            );
            #[cfg(feature = "zod-uri")]
            {
                let validate = compiled.unwrap();
                for value in [
                    " https://example.com ",
                    "https://example.com/a b",
                    "http://exa\tmple.com",
                    "mailto:user@example.com",
                ] {
                    assert!(validate(&json!(value)).is_ok(), "{value:?}");
                }
                for value in ["relative", "https://", "http://exa mple.com"] {
                    assert!(validate(&json!(value)).is_err(), "{value:?}");
                }
            }
        }
    }
}
