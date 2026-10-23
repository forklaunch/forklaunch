use ramhorns::Content;

use super::models::RustTypeAliasTemplate;

#[derive(Content, Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct SdkResponseVariant {
    pub shared_response_decoders: bool,
    pub media_discriminator: bool,
    pub headers_type: String,
    pub typescript_headers_type: String,
    pub header_fields: Vec<super::models::RustModelFieldTemplate>,
    pub header_rules: String,
    pub body_schema: String,
    pub body_schema_json: String,
    pub rust_body_schema: String,
    pub rust_header_rules: String,
    pub response_type: String,
    pub response_body_type: String,
    pub rust_response_type: String,
    pub status_code: u16,
    pub status_variant: String,
    pub empty_response: bool,
    pub json_response: bool,
    pub text_response: bool,
    pub binary_response: bool,
    pub multipart_response: bool,
    pub streaming_response: bool,
    pub response_kind: String,
    pub content_type: String,
}

#[derive(Content)]
#[allow(clippy::struct_excessive_bools)]
pub struct RustSdkTemplate {
    pub model_source: String,
    pub has_operations: bool,
    pub has_multipart: bool,
    pub has_multipart_response: bool,
    pub has_sse: bool,
    pub has_serde: bool,
    pub client_name: String,
    pub operations: Vec<RustOperationTemplate>,
}

#[derive(Content)]
#[allow(clippy::struct_excessive_bools)] // Template rendering uses independent feature sections.
pub struct RustOperationTemplate {
    pub has_body: bool,
    pub no_input: bool,
    pub shared_response_decoders: bool,
    pub version_metadata: String,
    pub request_objects: String,
    pub params_type: String,
    pub query_type: String,
    pub headers_type: String,
    pub typed_fields: bool,
    pub json_request_type: String,
    pub form_request: bool,
    pub request_type: String,
    pub responses_type: String,
    pub responses: Vec<SdkResponseVariant>,
    pub execute_method_name: String,
    pub method_name: String,
    pub http_method: String,
    pub path: String,
    pub binary_request: bool,
    pub multipart_request: bool,
    pub multipart_field: String,
    pub streaming_response: bool,
    pub request_content_type: String,
}

#[derive(Content)]
pub struct TypeScriptSdkTemplate {
    pub schema_models: Vec<RustTypeAliasTemplate>,
    pub form_stringify_source: String,
    pub transport_source: String,
    pub sse_source: String,
    pub operation_types: Vec<TypeScriptOperationTypeTemplate>,
    pub services: Vec<TypeScriptServiceTemplate>,
    pub aliases: Vec<TypeScriptAliasTemplate>,
}

#[derive(Content)]
pub struct TypeScriptOperationTypeTemplate {
    pub request_type: String,
    pub responses_type: String,
    pub request_params_type: String,
    pub request_query_type: String,
    pub request_body_type: String,
    pub request_body_optional: bool,
    pub request_headers_type: String,
    pub responses: Vec<SdkResponseVariant>,
}

#[derive(Content)]
pub struct TypeScriptServiceTemplate {
    pub service_key: String,
    pub routers: Vec<TypeScriptRouterTemplate>,
    pub last: bool,
}

#[derive(Content)]
pub struct TypeScriptRouterTemplate {
    pub router_key: String,
    pub operations: Vec<TypeScriptOperationTemplate>,
    pub last: bool,
}

#[derive(Content)]
pub struct TypeScriptOperationTemplate {
    pub version_metadata: String,
    pub request_objects: String,
    pub request_schema: String,
    pub operation_key: String,
    pub request_type: String,
    pub responses_type: String,
    pub optional_input: bool,
    pub service_name: String,
    pub router_name: String,
    pub operation_name: String,
    pub method: String,
    pub path: String,
    pub request_kind: String,
    pub request_content_type: String,
    pub responses: Vec<SdkResponseVariant>,
    pub last: bool,
}

#[derive(Content)]
pub struct TypeScriptAliasTemplate {
    pub alias: String,
    pub service_key: String,
}
