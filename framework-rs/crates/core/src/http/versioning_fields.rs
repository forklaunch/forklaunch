//! Parsed non-body fields for the selected version; wire values remain intact.
use super::{
    FieldValidator, SchemaValidationFailure, Variant, VersionBodyHooks, schema_execution_error,
};
use crate::http::{ContractValidationLocation, FrameworkRequest};
use serde_json::Value;
use std::collections::BTreeMap;

/// Value callbacks around a versioned parameter, query or header schema.
/// Absence and explicit JSON null remain distinct, as with body callbacks.
pub type VersionFieldHooks = VersionBodyHooks;

/// Values produced by the selected version's declared field schemas and hooks.
/// Parameters/query contain declared fields. Headers retain original wire keys;
/// only truthy transformed values replace existing headers, matching the TS
/// request parser. Defaults do not add previously absent header keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionFieldValues {
    pub params: BTreeMap<String, Value>,
    pub query: BTreeMap<String, Value>,
    pub headers: BTreeMap<String, Value>,
}

impl VersionFieldValues {
    /// Preserve existing wire header keys and overlay only truthy projected values.
    /// Matching uses exact lower-case wire keys, as the TypeScript request parser.
    #[must_use]
    pub fn project_wire_headers(&self, headers: &crate::http::Headers) -> BTreeMap<String, Value> {
        headers
            .iter()
            .map(|(key, raw)| {
                let value = self
                    .headers
                    .get(key)
                    .filter(|value| truthy(value))
                    .cloned()
                    .unwrap_or_else(|| Value::String(raw.clone()));
                (key.clone(), value)
            })
            .collect()
    }
}

impl Variant {
    pub(super) fn parse_fields(
        &self,
        request: &FrameworkRequest,
        parsed_query: Option<&BTreeMap<String, Value>>,
    ) -> Result<Option<VersionFieldValues>, crate::http::ContractValidationError> {
        let mut output = VersionFieldValues {
            headers: request
                .headers
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                .collect(),
            ..Default::default()
        };
        let mut valid = true;
        let mut group_headers = BTreeMap::new();
        for field in &self.fields {
            let raw = match field.location {
                ContractValidationLocation::PathParameter => {
                    request.params.get(&field.name).map(String::as_str)
                }
                ContractValidationLocation::Query => {
                    request.query.get(&field.name).map(String::as_str)
                }
                ContractValidationLocation::Header => {
                    crate::http::header_value(&request.headers, &field.name)
                }
                _ => unreachable!("only request fields are compiled"),
            };
            let raw = raw.map(|value| Value::String(value.to_owned()));
            let value = if field.location == ContractValidationLocation::Query {
                parsed_query.map_or(raw.as_ref(), |query| query.get(&field.name))
            } else {
                raw.as_ref()
            };
            let value = match field.parse(value) {
                Ok(value) => value,
                Err(SchemaValidationFailure::Invalid(_)) => {
                    valid = false;
                    continue;
                }
                Err(SchemaValidationFailure::Operational) => return Err(schema_execution_error()),
            };
            if let Some(value) = value {
                if field.location == ContractValidationLocation::Header {
                    group_headers.insert(field.name.clone(), value.clone());
                }
                match field.location {
                    ContractValidationLocation::PathParameter => {
                        output.params.insert(field.name.clone(), value);
                    }
                    ContractValidationLocation::Query => {
                        output.query.insert(field.name.clone(), value);
                    }
                    ContractValidationLocation::Header if truthy(&value) => {
                        for (key, target) in &mut output.headers {
                            if key.eq_ignore_ascii_case(&field.name) {
                                *target = value.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if valid {
            let groups = VersionFieldValues {
                headers: group_headers,
                ..output.clone()
            };
            valid = self.validate_groups(request, parsed_query, &groups)?;
        }
        Ok(valid.then_some(output))
    }
}

impl Variant {
    pub(super) fn validate_groups(
        &self,
        request: &FrameworkRequest,
        parsed_query: Option<&BTreeMap<String, Value>>,
        fields: &VersionFieldValues,
    ) -> Result<bool, crate::http::ContractValidationError> {
        let mut query: serde_json::Map<String, Value> = parsed_query.map_or_else(
            || {
                request
                    .query
                    .iter()
                    .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                    .collect()
            },
            |query| {
                query
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            },
        );
        query.extend(
            fields
                .query
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        let headers: serde_json::Map<String, Value> = self
            .route
            .request
            .headers
            .iter()
            .filter_map(|field| {
                fields
                    .headers
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(&field.name))
                    .map(|(_, value)| (field.name.clone(), value.clone()))
            })
            .collect();
        for (validator, value) in [
            (&self.query_group, Value::Object(query)),
            (&self.headers_group, Value::Object(headers)),
        ] {
            if let Some(validator) = validator {
                match validator(&value) {
                    Ok(()) => {}
                    Err(SchemaValidationFailure::Invalid(_)) => return Ok(false),
                    Err(SchemaValidationFailure::Operational) => {
                        return Err(schema_execution_error());
                    }
                }
            }
        }
        Ok(true)
    }
}

impl FieldValidator {
    // Errors reject this version; None is a successful absent output.
    fn parse(&self, raw: Option<&Value>) -> Result<Option<Value>, SchemaValidationFailure> {
        let candidate = self.hooks.preprocess_field(raw)?;
        if candidate.is_none() && self.required {
            return Err(SchemaValidationFailure::Invalid(String::new()));
        }
        if let (Some(validate), Some(value)) = (&self.validate, candidate.as_deref()) {
            validate(value)?;
        }
        self.hooks
            .transform(candidate)
            .map_err(|_| SchemaValidationFailure::Invalid(String::new()))
            .map(|value| value.map(std::borrow::Cow::into_owned))
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::Number(number) => number.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{
        FrameworkResponse, HttpMethod, LocalRouter, RequestContext, VersionBodyHooksRegistry,
    };
    use forklaunch_core::contract::{
        FieldContract, RequestContract, ResponseContract, RouteContract, VersionedContract,
    };
    use serde_json::json;

    fn contract() -> RouteContract {
        let mut field = FieldContract::required("count", "Count");
        field.schema.json_schema = Some(json!({"type":"integer","minimum":0}).to_string());
        RouteContract::get("Fields", "Fields", "/fields").with_version(VersionedContract {
            version: "one".into(),
            request: RequestContract {
                query: vec![field],
                ..Default::default()
            },
            responses: vec![ResponseContract::new(204, None)],
        })
    }

    #[test]
    fn checked_preprocessor_preserves_operational_failure() {
        let hooks = VersionFieldHooks::default()
            .with_checked_field_preprocess(|_| Err(SchemaValidationFailure::Operational));
        assert!(matches!(
            hooks.preprocess_field(None),
            Err(SchemaValidationFailure::Operational)
        ));
        let field = FieldValidator {
            location: ContractValidationLocation::Query,
            name: "count".into(),
            required: false,
            validate: None,
            hooks,
        };
        assert!(matches!(
            field.parse(None),
            Err(SchemaValidationFailure::Operational)
        ));
    }

    #[test]
    fn defaults_and_absent_outputs_reach_local_handlers_without_wire_mutation() {
        let route = contract();
        let mut hooks = VersionBodyHooksRegistry::default();
        hooks
            .insert_field(
                &route,
                "one",
                ContractValidationLocation::Query,
                "count",
                VersionFieldHooks::default()
                    .with_optional_preprocess(|_| Ok(Some(json!(9))))
                    .with_optional_transform(|value| {
                        assert_eq!(value, Some(&json!(9)));
                        Ok(None)
                    }),
            )
            .unwrap();
        let mut router = LocalRouter::new();
        router.register_with_body_hooks(route, &hooks, |request: FrameworkRequest| {
            assert!(request.query.is_empty());
            assert!(request.version.unwrap().parsed_fields.query.is_empty());
            Ok(FrameworkResponse::new(204))
        });
        let response = router
            .execute(FrameworkRequest::new(
                HttpMethod::Get,
                "/fields",
                RequestContext::new("fields"),
            ))
            .unwrap();
        assert_eq!(response.status_code, 204);
    }

    #[test]
    fn registration_rejects_unknown_fields_removed_versions_and_ambiguous_routes() {
        let route = contract();
        let mut hooks = VersionBodyHooksRegistry::default();
        for (version, location, name) in [
            ("missing", ContractValidationLocation::Query, "count"),
            ("one", ContractValidationLocation::Body, "count"),
            ("one", ContractValidationLocation::Query, "unknown"),
        ] {
            assert!(
                hooks
                    .insert_field(
                        &route,
                        version,
                        location,
                        name,
                        VersionFieldHooks::default()
                    )
                    .is_err()
            );
        }
        hooks
            .insert_field(
                &route,
                "one",
                ContractValidationLocation::Query,
                "count",
                VersionFieldHooks::default(),
            )
            .unwrap();
        assert!(hooks.validate_routes(&[]).is_err());
        assert!(
            hooks
                .validate_routes(&[route.clone(), route.clone()])
                .is_err()
        );
        let mut removed = route;
        removed.versions.clear();
        assert!(hooks.compile(&removed).is_err());
    }
}

#[cfg(test)]
mod group_tests {
    use super::*;
    use crate::{
        contract::{
            FieldContract, RequestContract, ResponseContract, RouteContract, SchemaRef,
            VersionedContract,
        },
        http::{HttpMethod, RequestContext, VersionedRoutePlan},
    };
    use serde_json::json;
    fn fields() -> Vec<FieldContract> {
        ["left", "right"]
            .into_iter()
            .map(|name| {
                let mut field = FieldContract::required(name, "String");
                field.required = false;
                field
            })
            .collect()
    }
    fn route() -> RouteContract {
        let schema=SchemaRef::named("Pair").with_json_schema(json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"left":{"type":"string"},"right":{"type":"string"}},"dependentRequired":{"left":["right"]},"anyOf":[{"required":["left"]},{"required":["right"]}],"additionalProperties":false}).to_string());
        RouteContract::get("groups", "groups", "/groups").with_version(VersionedContract {
            version: "one".into(),
            request: RequestContract {
                query: fields(),
                headers: fields(),
                query_schema: Some(schema.clone()),
                headers_schema: Some(schema),
                ..Default::default()
            },
            responses: vec![ResponseContract::new(204, None)],
        })
    }
    #[test]
    fn groups_validate_dependencies_unknown_query_and_case_insensitive_headers() {
        let plan = VersionedRoutePlan::compile(&route()).unwrap();
        for (query, headers, valid) in [
            (vec![("right", "yes")], vec![("RIGHT", "yes")], true),
            (
                vec![("left", "yes"), ("right", "yes")],
                vec![("Left", "yes"), ("RIGHT", "yes")],
                true,
            ),
            (vec![("left", "yes")], vec![("right", "yes")], false),
            (vec![("right", "yes")], vec![("left", "yes")], false),
            (
                vec![("right", "yes"), ("unknown", "no")],
                vec![("right", "yes")],
                false,
            ),
            (vec![], vec![("right", "yes")], false),
        ] {
            let mut request = FrameworkRequest::new(
                HttpMethod::Get,
                "/groups",
                RequestContext::new("group-proof"),
            );
            request.query = query
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect();
            request.headers = headers
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect();
            request
                .headers
                .insert("authorization".into(), "transport-secret".into());
            assert_eq!(plan.select(&request).is_ok(), valid);
        }
    }
    #[test]
    fn unsupported_or_missing_group_schema_fails_plan_compilation() {
        assert!(VersionedRoutePlan::compile(&route().for_version("one").unwrap()).is_err());
        for schema in [
            SchemaRef::named("Missing"),
            SchemaRef::named("Array").with_json_schema(r#"{"type":"array"}"#),
        ] {
            let mut route = route();
            route.versions.get_mut("one").unwrap().request.query_schema = Some(schema);
            assert!(VersionedRoutePlan::compile(&route).is_err());
        }
    }
}
