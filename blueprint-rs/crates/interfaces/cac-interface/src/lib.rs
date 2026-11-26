//! Code-set provider DTOs and contracts shared with the TypeScript CAC module.
#![forbid(unsafe_code)]
use forklaunch_core::domain::BlueprintResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::future::Future;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureCodeLookupDto {
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureCodeDto {
    pub code: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CodeSetType {
    Mock,
    Cpt,
}

/// Reports provider licensing metadata. A descriptor does not itself grant a
/// license or contain licensed codes; consumers must configure their provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodeSetDescriptorDto {
    pub code_set_type: CodeSetType,
    pub licensed: bool,
}

pub trait CodeSetProvider: Send + Sync {
    /// # Errors
    /// Returns provider/storage failures; unknown codes return `Ok(None)`.
    fn lookup_procedure_code(
        &self,
        lookup: ProcedureCodeLookupDto,
    ) -> impl Future<Output = BlueprintResult<Option<ProcedureCodeDto>>> + Send;
    fn describe(&self) -> CodeSetDescriptorDto;
}

/// Internal HMAC-protected code-set routes. This registry describes the lookup
/// provider surface only; the generated CAC module also requires protected
/// compliance export/erasure routes backed by its persistence service.
pub mod contracts {
    use super::{CodeSetDescriptorDto, ProcedureCodeDto};
    use forklaunch_core::contract::HmacAuth;
    use forklaunch_core::contract::{
        AccessLevel, AuthConfig, BodyContract, BodyKind, ContractRegistry, FieldContract,
        RouteContract, SchemaRef, ServiceContract,
    };

    fn json_body<T: schemars::JsonSchema>(name: &str) -> BodyContract {
        let mut body = BodyContract::json(name);
        let mut settings = schemars::generate::SchemaSettings::draft07();
        settings.inline_subschemas = true;
        body.schema.json_schema = Some(
            serde_json::to_string(&settings.into_generator().into_root_schema_for::<T>())
                .expect("JSON Schema serializes"),
        );
        body
    }

    #[must_use]
    pub fn registry() -> ContractRegistry {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("cac", "0.1.0"));
        registry.register(
            RouteContract::get(
                "Describe Code Set",
                "Reports which procedure code-set provider is active (mock vs. licensed CPT)",
                "/codeSet",
            )
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_response(
                200,
                Some(json_body::<CodeSetDescriptorDto>("CodeSetDescriptor")),
            ),
        );
        registry.register(
            RouteContract::get(
                "Lookup Procedure Code",
                "Looks up a procedure code against the currently active code-set provider",
                "/codeSet/:code",
            )
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_param(FieldContract::required("code", "String"))
            .with_response(200, Some(json_body::<ProcedureCodeDto>("ProcedureCode")))
            .with_response(
                404,
                Some(BodyContract {
                    kind: BodyKind::Text,
                    schema: SchemaRef::named("String").with_json_schema(r#"{"type":"string"}"#),
                    content_type: Some("text/plain".to_owned()),
                }),
            ),
        );
        registry
    }
    /// Complete current CAC skeleton surface, including empty-entity compliance
    /// routes and service health. Protected compliance uses system-role access.
    #[must_use]
    pub fn module_registry() -> ContractRegistry {
        use forklaunch_core::contract::{JwtAuthConfig, RbacConfig};
        let mut registry = registry();
        let export = BodyContract {
            kind: BodyKind::Json,
            schema: SchemaRef::named("ComplianceExport").with_json_schema(r#"{"type":"object","required":["userId","entities"],"properties":{"userId":{"type":"string"},"entities":{"type":"object","additionalProperties":true}}}"#),
            content_type: Some("application/json".to_owned()),
        };
        let erase = BodyContract {
            kind: BodyKind::Json,
            schema: SchemaRef::named("ComplianceErasure").with_json_schema(r#"{"type":"object","required":["entitiesAffected","recordsDeleted"],"properties":{"entitiesAffected":{"type":"array","items":{"type":"string"}},"recordsDeleted":{"type":"number"}}}"#),
            content_type: Some("application/json".to_owned()),
        };
        for (route, response) in [
            (
                RouteContract::get(
                    "Export User Data",
                    "Exports user data from registered CAC entities",
                    "/compliance/export/:userId",
                ),
                export,
            ),
            (
                RouteContract::delete(
                    "Erase User Data",
                    "Erases user data from registered CAC entities",
                    "/compliance/erase/:userId",
                ),
                erase,
            ),
        ] {
            registry.register(
                route
                    .with_access(AccessLevel::Protected)
                    .with_auth(AuthConfig::jwt(JwtAuthConfig::JwksUrl {
                        url: "${JWKS_PUBLIC_KEY_URL}".to_owned(),
                    }))
                    .with_rbac(RbacConfig {
                        allowed_roles: vec!["system".to_owned()],
                        ..RbacConfig::default()
                    })
                    .with_param(FieldContract::required("userId", "String"))
                    .with_response(200, Some(response))
                    .with_response(
                        404,
                        Some(BodyContract {
                            kind: BodyKind::Text,
                            schema: SchemaRef::named("String")
                                .with_json_schema(r#"{"type":"string"}"#),
                            content_type: Some("text/plain".to_owned()),
                        }),
                    ),
            );
        }
        registry.register(RouteContract::get("Get Health", "Reports service health", "/health")
            .with_response(200, Some(BodyContract { kind: BodyKind::Json,
                schema: SchemaRef::named("CacHealth").with_json_schema(r#"{"type":"object","required":["service","status"],"properties":{"service":{"type":"string"},"status":{"type":"string"}}}"#),
                content_type: Some("application/json".to_owned()) })));
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_names_and_route_security_match_the_typescript_provider() {
        let value = serde_json::to_value(CodeSetDescriptorDto {
            code_set_type: CodeSetType::Mock,
            licensed: false,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"codeSetType":"mock","licensed":false})
        );
        let registry = contracts::registry();
        assert_eq!(registry.routes().len(), 2);
        for route in registry.routes() {
            assert_eq!(
                route.access,
                forklaunch_core::contract::AccessLevel::Internal
            );
            assert!(matches!(
                route.auth,
                Some(forklaunch_core::contract::AuthConfig::Hmac(_))
            ));
            let schema: serde_json::Value = serde_json::from_str(
                route.responses[0]
                    .body
                    .as_ref()
                    .unwrap()
                    .schema
                    .json_schema
                    .as_ref()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(schema["type"], "object");
        }
    }
}
