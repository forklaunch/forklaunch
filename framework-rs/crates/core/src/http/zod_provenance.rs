use forklaunch_validator::typescript_coerce::{
    OrderedJsonDocument, ZodJsonOutcome, ZodJsonUnsupported,
};
use forklaunch_validator::zod_provenance::{
    CompiledZodProvenance, MAX_BYTES, MAX_NODES, ZodProvenanceError, admit_roots, source_schema,
};
use serde_json::Value;
/// Install built-in parsers from retained schema provenance for all versions.
/// No caller-written validator or diagnostic reconstruction is required. The
/// entire registration is atomic. Wire/query and pre-auth policies remain
/// explicit on the supplied registry; no authentication credentials are changed.
/// # Errors
/// Rejects absent/stale provenance, unsupported nodes, conflicts, and aggregate
/// source metadata exceeding16MiB or10,000 fields before compiling any node.
pub fn register_zod_version_parsers(
    registry: &mut crate::http::VersionBodyHooksRegistry,
    route: &forklaunch_core::contract::RouteContract,
) -> Result<(), String> {
    use crate::http::{VersionRequestParseFailure, VersionRequestParser, VersionRequestProjection};
    use serde_json::json;
    if route.versions.is_empty() {
        return Err("Zod version provenance requires declared versions".into());
    }
    admit_route_sources(route)?;
    let compiled = route.versions.values().map(|version| {
        let section = |fields: &[forklaunch_core::contract::FieldContract]| -> Result<Value, ZodProvenanceError> {
            let fields = fields.iter().map(|field| {
                let schema = source_schema(field.schema.json_schema.as_deref().ok_or(ZodProvenanceError::Missing)?)?;
                Ok(json!({"name":field.name,"schema":schema}))
            }).collect::<Result<Vec<_>, ZodProvenanceError>>()?;
            Ok(json!({"kind":"object","fields":fields,"unknown":"strip","catchall":{"kind":"never"}}))
        };
        let body = version.request.body.as_ref().map(|body| source_schema(body.schema.json_schema.as_deref().ok_or(ZodProvenanceError::Missing)?)).transpose()?.unwrap_or_else(||json!({"kind":"optional","schema":{"kind":"unknown"}}));
        let schema = json!({"kind":"object","unknown":"strip","catchall":{"kind":"never"},"fields":[
            {"name":"params","schema":section(&version.request.params)?},
            {"name":"headers","schema":section(&version.request.headers)?},
            {"name":"query","schema":section(&version.request.query)?},
            {"name":"body","schema":body}
        ]});
        CompiledZodProvenance::compile(json!({"version":1,"schema":schema})).map(|compiled| (version.version.clone(), compiled))
    }).collect::<Result<Vec<_>, _>>().map_err(|error|error.to_string())?;
    let mut next = registry.clone();
    for (version, compiled) in compiled {
        next.insert_request_parser(
            route,
            &version,
            VersionRequestParser::new(move |input| {
                let evaluated = (|| {
                    let mut values = Vec::new();
                    let mut overhead = 0usize;
                    for fields in [
                        &input.fields.params,
                        &input.fields.headers,
                        &input.fields.query,
                    ] {
                        if fields.len() > MAX_NODES.saturating_sub(values.len()) {
                            return Err(ZodJsonUnsupported::InputNodes);
                        }
                        for (key, value) in fields {
                            overhead = overhead.saturating_add(key.len());
                            values.push(value);
                        }
                    }
                    if let Some(body) = input.body {
                        values.push(body);
                    }
                    admit_roots(&values, overhead).map_err(|_| ZodJsonUnsupported::InputBytes)?;
                    if let Some(source) = &input.json_body {
                        let document = source
                            .cached(|text| {
                                let document = OrderedJsonDocument::parse(text.as_bytes())
                                    .map_err(|_| {
                                        crate::http::VersionRequestJsonError::Preparation
                                    })?;
                                let Value::Object(fields) = input.fields_json() else {
                                    return Err(crate::http::VersionRequestJsonError::Preparation);
                                };
                                document
                                    .embed_in_object("body", fields)
                                    .map_err(|_| crate::http::VersionRequestJsonError::Preparation)
                            })
                            .map_err(|_| ZodJsonUnsupported::InputBytes)?;
                        compiled.evaluate_ordered(document, &[])
                    } else {
                        compiled.evaluate(Some(&input.to_json()), &[])
                    }
                })();
                match evaluated {
                    Ok(ZodJsonOutcome::Valid(Some(value))) => {
                        VersionRequestProjection::from_json(value)
                    }
                    Ok(ZodJsonOutcome::Invalid(errors)) => {
                        Err(VersionRequestParseFailure::Validation(errors))
                    }
                    _ => Err(VersionRequestParseFailure::Operational),
                }
            }),
        )?;
    }
    next.compile(route)?;
    *registry = next;
    Ok(())
}
fn admit_route_sources(route: &forklaunch_core::contract::RouteContract) -> Result<(), String> {
    let mut bytes = MAX_BYTES;
    let mut fields = MAX_NODES;
    for version in route.versions.values() {
        for field in version
            .request
            .params
            .iter()
            .chain(&version.request.headers)
            .chain(&version.request.query)
        {
            fields = fields
                .checked_sub(1)
                .ok_or("Zod request provenance field budget exceeded")?;
            bytes = bytes
                .checked_sub(
                    field
                        .name
                        .len()
                        .saturating_add(field.schema.json_schema.as_ref().map_or(0, String::len)),
                )
                .ok_or("Zod request provenance byte budget exceeded")?;
        }
        if let Some(body) = &version.request.body {
            bytes = bytes
                .checked_sub(body.schema.json_schema.as_ref().map_or(0, String::len))
                .ok_or("Zod request provenance byte budget exceeded")?;
        }
    }
    Ok(())
}

/// Validator-backed DTO metadata adapted to the core route contract.
/// Pure validator users may export JSON Schema without linking HTTP or contracts.
pub trait FrameworkSchema: forklaunch_validator::FrameworkSchema {
    #[must_use]
    fn schema_ref() -> forklaunch_core::contract::SchemaRef {
        forklaunch_core::contract::SchemaRef::named(Self::schema_name())
            .with_json_schema(Self::json_schema_json())
    }
}
impl<T: forklaunch_validator::FrameworkSchema> FrameworkSchema for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_validator::zod_provenance::ZOD_DIAGNOSTICS_KEYWORD;
    use serde_json::json;
    #[test]
    fn artifact_binding_and_registration_are_atomic() {
        use forklaunch_core::contract::{
            BodyContract, RequestContract, RouteContract, SchemaRef, VersionedContract,
        };
        let projection = json!({"type":"string"});
        let mut artifact = projection.clone();
        artifact[ZOD_DIAGNOSTICS_KEYWORD] = json!({"version":1,"schema":{"kind":"string","checks":[]},"projection":projection.to_string()});
        let source = SchemaRef::named("Body").with_json_schema(artifact.to_string());
        assert_eq!(
            source_schema(source.json_schema.as_deref().unwrap()).unwrap()["kind"],
            "string"
        );
        let mut changed = artifact.clone();
        changed["minLength"] = json!(3);
        assert!(matches!(
            source_schema(&changed.to_string()),
            Err(ZodProvenanceError::Shape)
        ));
        let mut body = BodyContract::json("Body");
        body.schema = source;
        let mut route = RouteContract::post("Bound", "Bound", "/bound");
        for version in ["first", "last"] {
            route = route.with_version(VersionedContract {
                version: version.into(),
                request: RequestContract::default().with_body(body.clone()),
                responses: vec![],
            });
        }
        route
            .versions
            .get_mut("last")
            .unwrap()
            .request
            .body
            .as_mut()
            .unwrap()
            .schema = SchemaRef::named("Missing");
        let mut registry = crate::http::VersionBodyHooksRegistry::default();
        assert!(register_zod_version_parsers(&mut registry, &route).is_err());
        // A failed later source must not retain the first parser. The complete
        // corrected registration therefore succeeds without a duplicate conflict.
        route
            .versions
            .get_mut("last")
            .unwrap()
            .request
            .body
            .as_mut()
            .unwrap()
            .schema = body.schema;
        register_zod_version_parsers(&mut registry, &route).unwrap();
        assert!(registry.compile(&route).is_ok());
    }
}
