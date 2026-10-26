//! Bounded reusable JSON evaluator integrated with HTTP and mounted MCP transport.
#[path = "json_composition/contract.rs"]
mod contract;
use axum::{Extension, Json};
use forklaunch_axum::{
    AuthErrorFormat, CorrelationIdFormat, ForkLaunchRouter, JsonRequestParsers,
    JsonTransformFailure, ParsedJsonBody, RequestValidators, RouterConfig,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    BodyContract, BodyKind, ResponseContract, RouteContract, SchemaRef, ValidationMode,
};
use forklaunch_core::http::typescript_request_validation_error_response;
use forklaunch_core::http::{BodyParserHooks, ErrorKind, ErrorResponse, FrameworkRequest};
use forklaunch_validator::typescript_coerce;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn tuple_fields() -> Vec<typescript_coerce::ZodJsonField<'static>> {
    use typescript_coerce::{ZodJsonField as Field, ZodJsonSchema as Schema};
    ["pair", "trail"]
        .into_iter()
        .map(|name| Field {
            name,
            schema: Schema::Optional(Box::new(Schema::Tuple {
                items: vec![
                    Schema::String(vec![typescript_coerce::ZodStringCheck {
                        rule: typescript_coerce::ZodStringRule::Step(
                            typescript_coerce::ZodStringStep::Trim,
                        ),
                        message: None,
                    }]),
                    Schema::Number(vec![]),
                ],
                rest: (name == "trail").then(|| Box::new(Schema::Boolean)),
            })),
        })
        .collect()
}
fn runtime_format_field() -> typescript_coerce::ZodJsonField<'static> {
    use typescript_coerce::{
        ZodJsonField as Field, ZodJsonSchema as Schema, ZodStringCheck as Check,
        ZodStringFormat as Format, ZodStringRule as Rule,
    };
    let fields = [
        (
            "time",
            vec![
                Rule::Step(typescript_coerce::ZodStringStep::Trim),
                Rule::Format(Format::Time {
                    precision: typescript_coerce::ZodTimePrecision::new(Some(3)).unwrap(),
                }),
            ],
        ),
        (
            "jwt",
            vec![Rule::Jwt {
                algorithm: Some("HS256"),
            }],
        ),
        (
            "ip",
            vec![Rule::Format(Format::Ip(Some(
                typescript_coerce::ZodIpVersion::V6,
            )))],
        ),
        ("emoji", vec![Rule::Format(Format::Emoji)]),
    ]
    .into_iter()
    .map(|(name, rules)| Field {
        name,
        schema: Schema::String(
            rules
                .into_iter()
                .map(|rule| Check {
                    rule,
                    message: None,
                })
                .collect(),
        ),
    })
    .collect();
    Field {
        name: "formats",
        schema: Schema::Optional(Box::new(Schema::Object(fields))),
    }
}
fn intersection_fields() -> Vec<typescript_coerce::ZodJsonField<'static>> {
    use typescript_coerce::{
        ZodJsonField as Field, ZodJsonSchema as Schema, ZodStringCheck, ZodStringRule,
        ZodStringStep,
    };
    let trim = || {
        Schema::String(vec![ZodStringCheck {
            rule: ZodStringRule::Step(ZodStringStep::Trim),
            message: None,
        }])
    };
    vec![
        Field {
            name: "joined",
            schema: Schema::Optional(Box::new(Schema::Intersection {
                left: Box::new(Schema::Object(vec![Field {
                    name: "left",
                    schema: trim(),
                }])),
                right: Box::new(Schema::Object(vec![Field {
                    name: "right",
                    schema: Schema::Number(vec![]),
                }])),
            })),
        },
        Field {
            name: "conflict",
            schema: Schema::Optional(Box::new(Schema::Intersection {
                left: Box::new(trim()),
                right: Box::new(Schema::String(vec![])),
            })),
        },
    ]
}
fn discriminated_field() -> typescript_coerce::ZodJsonField<'static> {
    use typescript_coerce::{
        ZodJsonEnumValue as EnumValue, ZodJsonField as Field, ZodJsonNativeEnumEntry as Entry,
        ZodJsonSchema as Schema,
    };
    Field {
        name: "event",
        schema: Schema::Optional(Box::new(Schema::DiscriminatedUnion {
            discriminator: "kind",
            options: vec![
                Schema::Object(vec![
                    Field {
                        name: "kind",
                        schema: Schema::Enum(vec!["create", "update"]),
                    },
                    Field {
                        name: "label",
                        schema: Schema::String(vec![typescript_coerce::ZodStringCheck {
                            rule: typescript_coerce::ZodStringRule::Step(
                                typescript_coerce::ZodStringStep::Trim,
                            ),
                            message: None,
                        }]),
                    },
                ]),
                Schema::Object(vec![
                    Field {
                        name: "kind",
                        schema: Schema::NativeEnum(vec![
                            Entry {
                                name: "0",
                                value: EnumValue::String("A"),
                            },
                            Entry {
                                name: "A",
                                value: EnumValue::Number(0.0),
                            },
                        ]),
                    },
                    Field {
                        name: "count",
                        schema: Schema::Number(vec![]),
                    },
                ]),
            ],
        })),
    }
}
fn case_fields() -> Vec<typescript_coerce::ZodJsonField<'static>> {
    use typescript_coerce::{
        ZodJsonField as Field, ZodJsonSchema as Schema, ZodJsonStringCheck as Check,
        ZodStringCheck, ZodStringRule, ZodStringStep,
    };
    vec![
        Field {
            name: "caseText",
            schema: Schema::Optional(Box::new(Schema::CheckedString(vec![
                Check::Uppercase,
                Check::Scalar(ZodStringCheck {
                    rule: ZodStringRule::Step(ZodStringStep::Trim),
                    message: None,
                }),
                Check::Scalar(ZodStringCheck {
                    rule: ZodStringRule::Step(ZodStringStep::Max(2)),
                    message: None,
                }),
            ]))),
        },
        Field {
            name: "lowerText",
            schema: Schema::Optional(Box::new(Schema::CheckedString(vec![Check::Lowercase]))),
        },
    ]
}
fn array_length_fields() -> Vec<typescript_coerce::ZodJsonField<'static>> {
    use forklaunch_validator::zod_array::{ZodArrayCheck, ZodArrayRule};
    use typescript_coerce::{
        ZodFloatBound, ZodJsonField, ZodJsonSchema, ZodStringCheck, ZodStringRule, ZodStringStep,
    };
    vec![ZodJsonField {
        name: "boundedItems",
        schema: ZodJsonSchema::Optional(Box::new(ZodJsonSchema::CheckedArray {
            element: Box::new(ZodJsonSchema::String(vec![ZodStringCheck {
                rule: ZodStringRule::Step(ZodStringStep::Trim),
                message: None,
            }])),
            checks: vec![
                ZodArrayCheck {
                    rule: ZodArrayRule::Min(ZodFloatBound::new(4.0)),
                    message: Some("replaced"),
                },
                ZodArrayCheck {
                    rule: ZodArrayRule::Min(ZodFloatBound::new(1.5)),
                    message: Some("array minimum"),
                },
                ZodArrayCheck {
                    rule: ZodArrayRule::Max(ZodFloatBound::new(2.5)),
                    message: Some(""),
                },
                ZodArrayCheck {
                    rule: ZodArrayRule::Length(ZodFloatBound::new(f64::NAN)),
                    message: Some("inert"),
                },
            ],
        })),
    }]
}

fn float_length_fields() -> Vec<typescript_coerce::ZodJsonField<'static>> {
    use typescript_coerce::{
        ZodFloatBound as Bound, ZodJsonField as Field, ZodJsonSchema as Schema,
        ZodStringCheck as Check, ZodStringRule as Rule,
    };
    [
        (
            "floatText",
            vec![
                Rule::MinFloat(Bound::new(1.5)),
                Rule::MaxFloat(Bound::new(2.5)),
            ],
        ),
        ("nanText", vec![Rule::LengthFloat(Bound::new(f64::NAN))]),
    ]
    .into_iter()
    .map(|(name, rules)| Field {
        name,
        schema: Schema::Optional(Box::new(Schema::String(
            rules
                .into_iter()
                .map(|rule| Check {
                    rule,
                    message: None,
                })
                .collect(),
        ))),
    })
    .collect()
}
fn composition_schema(fallback: &Value) -> typescript_coerce::ZodJsonSchema<'_> {
    use typescript_coerce::{ZodJsonField as Field, ZodJsonSchema as Schema};
    let mut fields = vec![
        Field {
            name: "amount",
            schema: Schema::Number(vec![]),
        },
        Field {
            name: "label",
            schema: Schema::Default {
                schema: Box::new(Schema::String(vec![])),
                value: fallback,
            },
        },
        Field {
            name: "kind",
            schema: Schema::Optional(Box::new(Schema::Union(vec![
                Schema::Literal(typescript_coerce::ZodJsonLiteral::String("create")),
                Schema::Literal(typescript_coerce::ZodJsonLiteral::String("update")),
            ]))),
        },
        Field {
            name: "choice",
            schema: Schema::Optional(Box::new(Schema::Union(vec![
                Schema::String(vec![
                    typescript_coerce::ZodStringCheck {
                        rule: typescript_coerce::ZodStringRule::Step(
                            typescript_coerce::ZodStringStep::Trim,
                        ),
                        message: None,
                    },
                    typescript_coerce::ZodStringCheck {
                        rule: typescript_coerce::ZodStringRule::Step(
                            typescript_coerce::ZodStringStep::Min(3),
                        ),
                        message: None,
                    },
                ]),
                Schema::Number(vec![typescript_coerce::ZodNumberCheck {
                    step: typescript_coerce::ZodNumberStep::Int,
                    message: None,
                }]),
                Schema::Object(vec![Field {
                    name: "nested",
                    schema: Schema::Union(vec![
                        Schema::String(vec![]),
                        Schema::Array {
                            element: Box::new(Schema::Boolean),
                            checks: vec![],
                        },
                    ]),
                }]),
            ]))),
        },
        Field {
            name: "labels",
            schema: Schema::Optional(Box::new(Schema::Record {
                key: Box::new(Schema::String(vec![typescript_coerce::ZodStringCheck {
                    rule: typescript_coerce::ZodStringRule::Step(
                        typescript_coerce::ZodStringStep::Trim,
                    ),
                    message: None,
                }])),
                value: Box::new(Schema::Number(vec![])),
            })),
        },
        Field {
            name: "settings",
            schema: Schema::Optional(Box::new(Schema::StrictObject(vec![Field {
                name: "enabled",
                schema: Schema::Boolean,
            }]))),
        },
        Field {
            name: "items",
            schema: Schema::Optional(Box::new(Schema::Array {
                element: Box::new(Schema::Object(vec![Field {
                    name: "enabled",
                    schema: Schema::Boolean,
                }])),
                checks: vec![],
            })),
        },
    ];
    fields.extend(float_length_fields());
    fields.extend(array_length_fields());
    fields.extend(case_fields());
    fields.push(discriminated_field());
    fields.extend(tuple_fields());
    fields.push(runtime_format_field());
    fields.extend(intersection_fields());
    fields.extend(absence_fields());
    fields.extend(object_policy_fields());
    #[cfg(feature = "zod-uri")]
    fields.push(uri_field());
    Schema::Object(fields)
}
#[cfg(feature = "zod-uri")]
fn uri_field<'a>() -> typescript_coerce::ZodJsonField<'a> {
    use typescript_coerce::{ZodJsonField as Field, ZodJsonSchema as Schema};
    Field {
        name: "uri",
        schema: Schema::Optional(Box::new(Schema::String(vec![
            typescript_coerce::ZodStringCheck {
                rule: typescript_coerce::ZodStringRule::Format(
                    typescript_coerce::ZodStringFormat::Uri,
                ),
                message: None,
            },
        ]))),
    }
}

fn absence_fields<'a>() -> Vec<typescript_coerce::ZodJsonField<'a>> {
    use typescript_coerce::{ZodJsonField as Field, ZodJsonSchema as Schema};
    vec![
        Field {
            name: "cleared",
            schema: Schema::Optional(Box::new(Schema::Null)),
        },
        Field {
            name: "omitted",
            schema: Schema::Undefined,
        },
        Field {
            name: "voided",
            schema: Schema::Void,
        },
        Field {
            name: "forbidden",
            schema: Schema::Optional(Box::new(Schema::Never)),
        },
        Field {
            name: "nothing",
            schema: Schema::Union(vec![Schema::Void, Schema::Null, Schema::Undefined]),
        },
    ]
}

fn object_policy_fields<'a>() -> Vec<typescript_coerce::ZodJsonField<'a>> {
    use typescript_coerce::{
        ZodJsonField as Field, ZodJsonSchema as Schema, ZodJsonUnknownKeys as Policy,
    };
    vec![
        Field {
            name: "bag",
            schema: Schema::Optional(Box::new(Schema::ObjectWithPolicy {
                fields: vec![Field {
                    name: "label",
                    schema: Schema::String(vec![]),
                }],
                unknown: Policy::Passthrough,
                catchall: None,
            })),
        },
        Field {
            name: "scores",
            schema: Schema::Optional(Box::new(Schema::ObjectWithPolicy {
                fields: vec![Field {
                    name: "label",
                    schema: Schema::String(vec![]),
                }],
                unknown: Policy::Strict,
                catchall: Some(Box::new(Schema::Number(vec![]))),
            })),
        },
    ]
}

fn transform(
    request: &FrameworkRequest,
    root_default: bool,
) -> Result<Option<Value>, JsonTransformFailure> {
    use typescript_coerce::{ZodJsonOutcome, ZodJsonSchema as Schema};
    let fallback = json!("fallback");
    let schema = composition_schema(&fallback);
    let root_value = json!({"amount":7,"label":"fallback"});
    let schema = if root_default {
        Schema::Default {
            schema: Box::new(schema),
            value: &root_value,
        }
    } else {
        schema
    };
    let value = request
        .parsed_json
        .as_ref()
        .and_then(|parsed| parsed.value.as_deref());
    // This fixture registers no reviver or preceding transformation. Only at
    // that explicit boundary may decompressed bytes reconstruct original order.
    // The parser's framed-empty synthetic object has no own-key ambiguity.
    let result = if let Some(raw) = request
        .parsed_json
        .as_ref()
        .and_then(|parsed| parsed.raw_body.as_deref())
        .filter(|raw| value.is_some() && !raw.is_empty())
    {
        let document = typescript_coerce::OrderedJsonDocument::parse(raw)
            .map_err(|_| JsonTransformFailure::Operational)?;
        typescript_coerce::zod_json_ordered_at_path(&document, &schema, &["body"])
    } else {
        typescript_coerce::zod_json_at_path(value, &schema, &["body"])
    };
    match result {
        Ok(ZodJsonOutcome::Valid(value)) => Ok(value),
        Ok(ZodJsonOutcome::Invalid(errors)) => Err(JsonTransformFailure::Validation(
            typescript_request_validation_error_response(
                Some(&errors),
                Some(&request.context.correlation_id),
            ),
        )),
        Err(_) => Err(JsonTransformFailure::Operational),
    }
}
fn report(parsed: ParsedJsonBody) -> Value {
    let mut result = json!({"hasBody":parsed.value.is_some()});
    if let Some(value) = parsed.value {
        result["body"] = value.as_ref().clone();
    }
    if let Some(raw) = parsed.raw_body {
        result["raw"] = json!(String::from_utf8_lossy(&raw));
    }
    result
}
fn rejection() -> ResponseContract {
    ResponseContract::new(
        400,
        Some(BodyContract {
            kind: BodyKind::Text,
            schema: SchemaRef::named("String"),
            content_type: Some("text/plain".into()),
        }),
    )
}
#[tokio::main]
async fn main() {
    let calls = Arc::new(AtomicUsize::new(0));
    let transforms = Arc::new(AtomicUsize::new(0));
    let authentications = Arc::new(AtomicUsize::new(0));
    let mut parsers = JsonRequestParsers::default();
    let mut validators = RequestValidators::default();
    let mut router = ForkLaunchRouter::new(ServiceInfo::new("transform", "1"));
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ] {
        for profile in ["public", "protected", "default", "remove", "unsupported"] {
            let route = contract::route(name, mode, profile);
            parsers.insert(&route, BodyParserHooks::default()).unwrap();
            let transform_calls = transforms.clone();
            validators
                .insert_json_transform_checked(&route, &parsers, rejection(), move |request| {
                    transform_calls.fetch_add(1, Ordering::SeqCst);
                    match profile {
                        "remove" => Ok(None),
                        "unsupported" => {
                            let schema = typescript_coerce::ZodJsonSchema::Object(vec![
                                typescript_coerce::ZodJsonField {
                                    name: "__proto__",
                                    schema: typescript_coerce::ZodJsonSchema::Unknown,
                                },
                            ]);
                            assert!(typescript_coerce::zod_json(None, &schema).is_err());
                            Err(JsonTransformFailure::Operational)
                        }
                        _ => transform(request, profile == "default"),
                    }
                })
                .unwrap();
            let count = calls.clone();
            router = router.post(
                route,
                move |Extension(parsed): Extension<ParsedJsonBody>| async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Json(report(parsed))
                },
            );
        }
    }
    let auth_count = authentications.clone();
    let app = router
        .get(RouteContract::get("Counters", "Counters", "/counters").with_response(200,None), move || {
            let value=json!({"transforms":transforms.load(Ordering::SeqCst),"auth":authentications.load(Ordering::SeqCst)});
            async move {Json(value)}
        })
        .get(
            RouteContract::get("Calls", "Calls", "/calls").with_response(200, None),
            move || async move { Json(json!({"handlerCalls":calls.load(Ordering::SeqCst)})) },
        )
        .with_config(
            RouterConfig {
                request_validators: validators,
                json_request_parsers: parsers,
                auth_error_format: AuthErrorFormat::TypeScriptText,
                correlation_id_format: CorrelationIdFormat::TypeScript,
                ..RouterConfig::default()
            }
            .with_response_authenticator(move |_, _| {
                auth_count.fetch_add(1, Ordering::SeqCst);
                async {
                Err(ErrorResponse::new(
                    ErrorKind::Unauthorized,
                    "Authorization token required.",
                ))
                }
            }),
        );
    #[cfg(feature = "mcp")]
    let app = app
        .into_router_with_mcp(forklaunch_core::mcp::server::McpServerOptions::new(
            "composition",
            "1",
        ))
        .unwrap();
    #[cfg(not(feature = "mcp"))]
    let app = app.into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
