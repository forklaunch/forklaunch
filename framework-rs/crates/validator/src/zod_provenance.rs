//! Explicit source provenance lowering into the existing bounded Zod evaluator.
//! This never guesses source validator kinds from generic JSON Schema keywords.
use crate::typescript_coerce::{
    OrderedJsonDocument, ZodFloatBound, ZodIpVersion, ZodJsonEnumValue, ZodJsonField,
    ZodJsonLiteral, ZodJsonNativeEnumEntry, ZodJsonOutcome, ZodJsonSchema, ZodJsonUnknownKeys,
    ZodJsonUnsupported, ZodMultipleOf, ZodNumberCheck, ZodNumberStep, ZodStringCheck,
    ZodStringFormat, ZodStringRule, ZodStringStep, ZodTimePrecision, zod_json_at_path,
    zod_json_ordered_at_path,
};
use crate::zod_array::{ZodArrayCheck, ZodArrayRule};
use serde_json::Value;

pub const ZOD_DIAGNOSTICS_KEYWORD: &str = "x-forklaunch-zod-diagnostics";
pub const MAX_NODES: usize = 10_000;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

/// Registration failures are operational/configuration failures, never client errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZodProvenanceError {
    Missing,
    Version,
    Shape,
    Unsupported,
    Budget,
    Default,
}
impl std::fmt::Display for ZodProvenanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unsupported or invalid Zod diagnostic provenance: {self:?}"
        )
    }
}
impl std::error::Error for ZodProvenanceError {}

/// Immutable admitted source metadata. Lowering borrows strings/default values
/// and allocates only bounded schema structure; all evaluation uses `zod_json`.
#[derive(Debug, Clone)]
pub struct CompiledZodProvenance {
    schema: Value,
}
impl CompiledZodProvenance {
    /// Compile an explicit provenance envelope, not an arbitrary JSON Schema.
    /// # Errors
    /// Rejects unsupported versions/nodes, malformed fields and resource excess.
    pub fn compile(value: Value) -> Result<Self, ZodProvenanceError> {
        admit(&value)?;
        if value.get("version").and_then(Value::as_u64) != Some(1) {
            return Err(ZodProvenanceError::Version);
        }
        let Value::Object(mut object) = value else {
            return Err(ZodProvenanceError::Shape);
        };
        let schema = object.remove("schema").ok_or(ZodProvenanceError::Shape)?;
        let result = Self { schema };
        let borrowed = lower(&result.schema)?;
        // Runs only pure, bounded built-in admission. No application callbacks.
        zod_json_at_path(None, &borrowed, &[]).map_err(|_| ZodProvenanceError::Unsupported)?;
        validate_defaults(&result.schema)?;
        Ok(result)
    }
    /// Evaluate an immutable ordered JSON document without reconstructing order
    /// from a possibly transformed value.
    /// # Errors
    /// Runtime resource/representation failures stay distinct from parse errors.
    pub fn evaluate_ordered(
        &self,
        input: &OrderedJsonDocument,
        path: &[&str],
    ) -> Result<ZodJsonOutcome, ZodJsonUnsupported> {
        let schema = lower(&self.schema).map_err(|_| ZodJsonUnsupported::SchemaNodes)?;
        zod_json_ordered_at_path(input, &schema, path)
    }
    /// # Errors
    /// Runtime resource/representation failures stay distinct from parse errors.
    pub fn evaluate(
        &self,
        input: Option<&Value>,
        path: &[&str],
    ) -> Result<ZodJsonOutcome, ZodJsonUnsupported> {
        let schema = lower(&self.schema).map_err(|_| ZodJsonUnsupported::SchemaNodes)?;
        zod_json_at_path(input, &schema, path)
    }
}
fn admit(root: &Value) -> Result<(), ZodProvenanceError> {
    admit_roots(&[root], 0)
}
/// Admit aggregate input under the evaluator resource budget.
/// # Errors
/// Rejects excessive depth, nodes, or bytes before evaluation.
pub fn admit_roots(roots: &[&Value], overhead: usize) -> Result<(), ZodProvenanceError> {
    if roots.len() > MAX_NODES {
        return Err(ZodProvenanceError::Budget);
    }
    let mut stack = roots
        .iter()
        .map(|value| (*value, 0usize))
        .collect::<Vec<_>>();
    let mut nodes = MAX_NODES;
    let mut bytes = MAX_BYTES
        .checked_sub(overhead)
        .ok_or(ZodProvenanceError::Budget)?;
    while let Some((value, depth)) = stack.pop() {
        nodes = nodes.checked_sub(1).ok_or(ZodProvenanceError::Budget)?;
        if depth > 64 {
            return Err(ZodProvenanceError::Budget);
        }
        let charge = match value {
            Value::String(value) => value.len(),
            Value::Array(values) => {
                if values.len() > nodes.saturating_sub(stack.len()) {
                    return Err(ZodProvenanceError::Budget);
                }
                stack.extend(values.iter().map(|value| (value, depth + 1)));
                values.len().saturating_mul(32)
            }
            Value::Object(values) => {
                if values.len() > nodes.saturating_sub(stack.len()) {
                    return Err(ZodProvenanceError::Budget);
                }
                let mut size = values.len().saturating_mul(64);
                for (key, value) in values {
                    size = size.saturating_add(key.len());
                    stack.push((value, depth + 1));
                }
                size
            }
            _ => 32,
        };
        bytes = bytes
            .checked_sub(charge)
            .ok_or(ZodProvenanceError::Budget)?;
    }
    Ok(())
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, ZodProvenanceError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(ZodProvenanceError::Shape)
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value], ZodProvenanceError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(ZodProvenanceError::Shape)
}
fn member<'a>(value: &'a Value, key: &str) -> Result<&'a Value, ZodProvenanceError> {
    value.get(key).ok_or(ZodProvenanceError::Shape)
}
fn number(value: &Value) -> Result<f64, ZodProvenanceError> {
    if let Some(value) = value.as_f64().filter(|value| value.is_finite()) {
        return Ok(value);
    }
    match value.as_str() {
        Some("NaN") => Ok(f64::NAN),
        Some("Infinity") => Ok(f64::INFINITY),
        Some("-Infinity") => Ok(f64::NEG_INFINITY),
        _ => Err(ZodProvenanceError::Shape),
    }
}
fn message(value: &Value) -> Result<Option<&str>, ZodProvenanceError> {
    value
        .get("message")
        .map(|value| value.as_str().ok_or(ZodProvenanceError::Shape))
        .transpose()
}
fn literal(value: &Value) -> Result<ZodJsonSchema<'_>, ZodProvenanceError> {
    Ok(match string(value, "kind")? {
        "null" => ZodJsonSchema::Literal(ZodJsonLiteral::Null),
        "undefined" => ZodJsonSchema::Literal(ZodJsonLiteral::Undefined),
        "string" => ZodJsonSchema::Literal(ZodJsonLiteral::String(string(value, "value")?)),
        "boolean" => ZodJsonSchema::Literal(ZodJsonLiteral::Boolean(
            member(value, "value")?
                .as_bool()
                .ok_or(ZodProvenanceError::Shape)?,
        )),
        "number" => {
            ZodJsonSchema::Literal(ZodJsonLiteral::Number(number(member(value, "value")?)?))
        }
        _ => return Err(ZodProvenanceError::Unsupported),
    })
}
fn lower(value: &Value) -> Result<ZodJsonSchema<'_>, ZodProvenanceError> {
    let child = |key| lower(member(value, key)?).map(Box::new);
    Ok(match string(value, "kind")? {
        "unknown" => ZodJsonSchema::Unknown,
        "null" => ZodJsonSchema::Null,
        "undefined" => ZodJsonSchema::Undefined,
        "void" => ZodJsonSchema::Void,
        "never" => ZodJsonSchema::Never,
        "strictBoolean" => ZodJsonSchema::StrictBoolean,
        "strictNumber" => ZodJsonSchema::StrictNumber(number_checks(value)?),
        "string" => ZodJsonSchema::String(string_checks(value)?),
        "literal" => literal(member(value, "literal")?)?,
        "optional" => ZodJsonSchema::Optional(child("schema")?),
        "nullable" => ZodJsonSchema::Nullable(child("schema")?),
        "default" => ZodJsonSchema::Default {
            schema: child("schema")?,
            value: member(value, "value")?,
        },
        "enum" => ZodJsonSchema::Enum(
            array(value, "values")?
                .iter()
                .map(|value| value.as_str().ok_or(ZodProvenanceError::Shape))
                .collect::<Result<_, _>>()?,
        ),
        "nativeEnum" => ZodJsonSchema::NativeEnum(
            array(value, "entries")?
                .iter()
                .map(|entry| {
                    let literal = member(entry, "literal")?;
                    Ok(ZodJsonNativeEnumEntry {
                        name: string(entry, "name")?,
                        value: match string(literal, "kind")? {
                            "string" => ZodJsonEnumValue::String(string(literal, "value")?),
                            "number" => {
                                ZodJsonEnumValue::Number(number(member(literal, "value")?)?)
                            }
                            _ => return Err(ZodProvenanceError::Unsupported),
                        },
                    })
                })
                .collect::<Result<_, _>>()?,
        ),
        "object" => ZodJsonSchema::ObjectWithPolicy {
            fields: array(value, "fields")?
                .iter()
                .map(|field| {
                    Ok(ZodJsonField {
                        name: string(field, "name")?,
                        schema: lower(member(field, "schema")?)?,
                    })
                })
                .collect::<Result<_, _>>()?,
            unknown: match string(value, "unknown")? {
                "strip" => ZodJsonUnknownKeys::Strip,
                "strict" => ZodJsonUnknownKeys::Strict,
                "passthrough" => ZodJsonUnknownKeys::Passthrough,
                _ => return Err(ZodProvenanceError::Shape),
            },
            catchall: Some(child("catchall")?),
        },
        "array" => ZodJsonSchema::CheckedArray {
            element: child("element")?,
            checks: array_checks(value)?,
        },
        "tuple" => ZodJsonSchema::Tuple {
            items: array(value, "items")?
                .iter()
                .map(lower)
                .collect::<Result<_, _>>()?,
            rest: if member(value, "rest")?.is_null() {
                None
            } else {
                Some(child("rest")?)
            },
        },
        "union" => ZodJsonSchema::Union(
            array(value, "options")?
                .iter()
                .map(lower)
                .collect::<Result<_, _>>()?,
        ),
        "discriminatedUnion" => ZodJsonSchema::DiscriminatedUnion {
            discriminator: string(value, "discriminator")?,
            options: array(value, "options")?
                .iter()
                .map(lower)
                .collect::<Result<_, _>>()?,
        },
        "intersection" => ZodJsonSchema::Intersection {
            left: child("left")?,
            right: child("right")?,
        },
        "record" => ZodJsonSchema::Record {
            key: child("key")?,
            value: child("value")?,
        },
        _ => return Err(ZodProvenanceError::Unsupported),
    })
}
fn number_checks(value: &Value) -> Result<Vec<ZodNumberCheck<'_>>, ZodProvenanceError> {
    array(value, "checks")?
        .iter()
        .map(|check| {
            let bound = || number(member(check, "value")?).map(ZodFloatBound::new);
            Ok(ZodNumberCheck {
                message: message(check)?,
                step: match string(check, "kind")? {
                    "int" => ZodNumberStep::Int,
                    "finite" => ZodNumberStep::Finite,
                    "min" => {
                        if member(check, "inclusive")?
                            .as_bool()
                            .ok_or(ZodProvenanceError::Shape)?
                        {
                            ZodNumberStep::MinFloat(bound()?)
                        } else {
                            ZodNumberStep::GtFloat(bound()?)
                        }
                    }
                    "max" => {
                        if member(check, "inclusive")?
                            .as_bool()
                            .ok_or(ZodProvenanceError::Shape)?
                        {
                            ZodNumberStep::MaxFloat(bound()?)
                        } else {
                            ZodNumberStep::LtFloat(bound()?)
                        }
                    }
                    "multipleOf" => ZodNumberStep::MultipleOf(ZodMultipleOf::new(number(member(
                        check, "value",
                    )?)?)),
                    _ => return Err(ZodProvenanceError::Unsupported),
                },
            })
        })
        .collect()
}
fn array_checks(value: &Value) -> Result<Vec<ZodArrayCheck<'_>>, ZodProvenanceError> {
    array(value, "checks")?
        .iter()
        .map(|check| {
            let bound = ZodFloatBound::new(number(member(check, "value")?)?);
            Ok(ZodArrayCheck {
                message: message(check)?,
                rule: match string(check, "kind")? {
                    "exactLength" => ZodArrayRule::Length(bound),
                    "minLength" => ZodArrayRule::Min(bound),
                    "maxLength" => ZodArrayRule::Max(bound),
                    _ => return Err(ZodProvenanceError::Unsupported),
                },
            })
        })
        .collect()
}
fn string_checks(value: &Value) -> Result<Vec<ZodStringCheck<'_>>, ZodProvenanceError> {
    array(value, "checks")?
        .iter()
        .map(|check| {
            let bound = || number(member(check, "value")?).map(ZodFloatBound::new);
            Ok(ZodStringCheck {
                message: message(check)?,
                rule: match string(check, "kind")? {
                    "min" => ZodStringRule::MinFloat(bound()?),
                    "max" => ZodStringRule::MaxFloat(bound()?),
                    "length" => ZodStringRule::LengthFloat(bound()?),
                    "trim" => ZodStringRule::Step(ZodStringStep::Trim),
                    "startsWith" => ZodStringRule::StartsWith(string(check, "value")?),
                    "endsWith" => ZodStringRule::EndsWith(string(check, "value")?),
                    "includes" => ZodStringRule::Includes {
                        needle: string(check, "value")?,
                        position: check.get("position").map(number).transpose()?,
                    },
                    "jwt" => ZodStringRule::Jwt {
                        algorithm: check
                            .get("alg")
                            .map(|value| value.as_str().ok_or(ZodProvenanceError::Shape))
                            .transpose()?,
                    },
                    kind => ZodStringRule::Format(format(kind, check)?),
                },
            })
        })
        .collect()
}
fn format(kind: &str, check: &Value) -> Result<ZodStringFormat, ZodProvenanceError> {
    let precision = || {
        let value = check
            .get("precision")
            .filter(|value| !value.is_null())
            .map(|value| value.as_i64().ok_or(ZodProvenanceError::Unsupported))
            .transpose()?;
        ZodTimePrecision::new(value).map_err(|_| ZodProvenanceError::Unsupported)
    };
    let version = || {
        check
            .get("version")
            .map(|value| {
                ZodIpVersion::try_from(value.as_str().ok_or(ZodProvenanceError::Shape)?)
                    .map_err(|_| ZodProvenanceError::Unsupported)
            })
            .transpose()
    };
    Ok(match kind {
        "uuid" => ZodStringFormat::Uuid,
        "email" => ZodStringFormat::Email,
        "cuid" => ZodStringFormat::Cuid,
        "cuid2" => ZodStringFormat::Cuid2,
        "ulid" => ZodStringFormat::Ulid,
        "nanoid" => ZodStringFormat::Nanoid,
        "base64" => ZodStringFormat::Base64,
        "base64url" => ZodStringFormat::Base64Url,
        "date" => ZodStringFormat::Date,
        "duration" => ZodStringFormat::Duration,
        "emoji" => ZodStringFormat::Emoji,
        "ip" => ZodStringFormat::Ip(version()?),
        "cidr" => ZodStringFormat::Cidr(version()?),
        "time" => ZodStringFormat::Time {
            precision: precision()?,
        },
        "datetime" => ZodStringFormat::DateTime {
            precision: precision()?,
            local: member(check, "local")?
                .as_bool()
                .ok_or(ZodProvenanceError::Shape)?,
            offset: member(check, "offset")?
                .as_bool()
                .ok_or(ZodProvenanceError::Shape)?,
        },
        #[cfg(feature = "zod-uri")]
        "url" => ZodStringFormat::Uri,
        _ => return Err(ZodProvenanceError::Unsupported),
    })
}
fn validate_defaults(root: &Value) -> Result<(), ZodProvenanceError> {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.get("kind").and_then(Value::as_str) == Some("default") {
            let schema = lower(member(node, "schema")?)?;
            if !matches!(
                zod_json_at_path(Some(member(node, "value")?), &schema, &[]),
                Ok(ZodJsonOutcome::Valid(Some(_)))
            ) {
                return Err(ZodProvenanceError::Default);
            }
        }
        match node.get("kind").and_then(Value::as_str) {
            Some("optional" | "nullable" | "default") => stack.push(member(node, "schema")?),
            Some("array") => stack.push(member(node, "element")?),
            Some("tuple") => {
                stack.extend(array(node, "items")?);
                if !member(node, "rest")?.is_null() {
                    stack.push(member(node, "rest")?);
                }
            }
            Some("union" | "discriminatedUnion") => stack.extend(array(node, "options")?),
            Some("intersection") => {
                stack.push(member(node, "left")?);
                stack.push(member(node, "right")?);
            }
            Some("record") => {
                stack.push(member(node, "key")?);
                stack.push(member(node, "value")?);
            }
            Some("object") => {
                for field in array(node, "fields")? {
                    stack.push(member(field, "schema")?);
                }
                stack.push(member(node, "catchall")?);
            }
            _ => {}
        }
    }
    Ok(())
}

/// Recover verified source provenance from a JSON Schema artifact.
/// # Errors
/// Rejects malformed, stale, unsupported, or oversized provenance.
pub fn source_schema(source: &str) -> Result<Value, ZodProvenanceError> {
    if source.len() > MAX_BYTES {
        return Err(ZodProvenanceError::Budget);
    }
    let Value::Object(mut artifact) =
        serde_json::from_str(source).map_err(|_| ZodProvenanceError::Shape)?
    else {
        return Err(ZodProvenanceError::Shape);
    };
    let provenance = artifact
        .remove(ZOD_DIAGNOSTICS_KEYWORD)
        .ok_or(ZodProvenanceError::Missing)?;
    admit(&provenance)?;
    let projection = string(&provenance, "projection")?;
    if projection.len() > MAX_BYTES {
        return Err(ZodProvenanceError::Budget);
    }
    let expected: Value =
        serde_json::from_str(projection).map_err(|_| ZodProvenanceError::Shape)?;
    if expected != Value::Object(artifact) {
        return Err(ZodProvenanceError::Shape);
    }
    CompiledZodProvenance::compile(provenance).map(|compiled| compiled.schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn compiled(schema: Value) -> CompiledZodProvenance {
        CompiledZodProvenance::compile(Value::Object(
            [("version".into(), json!(1)), ("schema".into(), schema)]
                .into_iter()
                .collect(),
        ))
        .unwrap()
    }
    #[test]
    fn strict_scalars_do_not_reuse_factory_coercion() {
        for (schema, accepted, rejected) in [
            (json!({"kind":"strictBoolean"}), json!(true), json!("true")),
            (
                json!({"kind":"strictNumber","checks":[]}),
                json!(2),
                json!("2"),
            ),
        ] {
            let schema = compiled(schema);
            assert!(matches!(
                schema.evaluate(Some(&accepted), &[]),
                Ok(ZodJsonOutcome::Valid(_))
            ));
            assert!(matches!(
                schema.evaluate(Some(&rejected), &[]),
                Ok(ZodJsonOutcome::Invalid(_))
            ));
            let Ok(ZodJsonOutcome::Invalid(errors)) = schema.evaluate(None, &["body"]) else {
                panic!("missing required diagnostic")
            };
            assert_eq!(errors[0].message, "Required");
            assert_eq!(errors[0].path, ["body"]);
        }
    }
    #[test]
    fn declared_default_validation_does_not_interpret_default_data_as_schema() {
        let value = json!({"kind":"default","schema":"ordinary data","value":12.0});
        let schema = compiled(json!({"kind":"default","schema":{"kind":"unknown"},"value":value}));
        assert_eq!(
            schema.evaluate(None, &[]).unwrap(),
            ZodJsonOutcome::Valid(Some(value))
        );
        assert!(matches!(
            CompiledZodProvenance::compile(
                json!({"version":1,"schema":{"kind":"default","schema":{"kind":"string","checks":[]},"value":12}})
            ),
            Err(ZodProvenanceError::Default)
        ));
    }
    #[test]
    fn ordered_document_preserves_extra_key_diagnostics_under_request_body() {
        let schema = compiled(
            json!({"kind":"object","unknown":"strict","fields":[],"catchall":{"kind":"never"}}),
        );
        let raw = OrderedJsonDocument::parse(br#"{"z":1,"a":2,"10":3,"2":4}"#).unwrap();
        let Ok(ZodJsonOutcome::Invalid(errors)) = schema.evaluate_ordered(&raw, &["body"]) else {
            panic!("expected strict error")
        };
        assert_eq!(
            errors[0].message,
            "Unrecognized key(s) in object: '2', '10', 'z', 'a'"
        );
        assert_eq!(errors[0].path, ["body"]);
        let root = compiled(
            json!({"kind":"object","unknown":"strip","catchall":{"kind":"never"},"fields":[{"name":"body","schema":schema.schema}]}),
        );
        let wrapped = raw.embed_in_object("body", serde_json::Map::new()).unwrap();
        assert_eq!(
            root.evaluate_ordered(&wrapped, &[]).unwrap(),
            ZodJsonOutcome::Invalid(errors)
        );
    }
    #[test]
    fn envelopes_and_source_projection_are_admitted_before_execution() {
        assert!(matches!(
            CompiledZodProvenance::compile(json!({"version":2,"schema":{"kind":"unknown"}})),
            Err(ZodProvenanceError::Version)
        ));
        assert!(matches!(
            CompiledZodProvenance::compile(json!({"version":1,"schema":{"kind":"callback"}})),
            Err(ZodProvenanceError::Unsupported)
        ));
        let oversized = json!({"version":1,"schema":{"kind":"literal","literal":{"kind":"string","value":"x".repeat(MAX_BYTES+1)}}});
        assert!(matches!(
            CompiledZodProvenance::compile(oversized),
            Err(ZodProvenanceError::Budget)
        ));
    }
}
