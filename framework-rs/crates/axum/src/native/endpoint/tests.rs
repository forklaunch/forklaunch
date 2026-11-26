use super::*;
use forklaunch_core::contract::HttpMethod;
use tower::ServiceExt;
#[forklaunch_core::contract::schema]
struct Note {
    #[field(min_length = 3)]
    title: String,
}
fn endpoint() -> Endpoint<JsonBody<Note>, JsonBody<Note>> {
    Endpoint::new(
        "Create note",
        "Create note",
        HttpMethod::Post,
        "/notes",
        201,
    )
    .unwrap()
}
fn registry(route: RouteContract) -> ContractRegistry {
    let mut registry = ContractRegistry::new();
    registry.register(route);
    registry
}
#[tokio::test]
async fn enforces_wire_schema_both_directions_without_leaking_values() {
    let endpoint = self::endpoint();
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind(endpoint, |_, note| async { Ok(note) })
        .unwrap()
        .finish()
        .unwrap();
    for (body, content, status) in [
        (r#"{"title":"hello"}"#, "application/json", 201),
        (r#"{"title":"x"}"#, "application/json", 400),
        (r#"{"title":"secret"}"#, "text/plain", 415),
        ("private malformed", "application/json", 400),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/notes")
                    .header("content-type", content)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        let bytes = to_bytes(response.into_body(), 10000).await.unwrap();
        if status != 201 {
            assert!(!String::from_utf8_lossy(&bytes).contains("private"));
        }
    }
    let endpoint = self::endpoint();
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind(endpoint, |_, _| async { Ok(Note { title: "x".into() }) })
        .unwrap()
        .finish()
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/notes")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"valid"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
}
#[test]
fn rejects_contract_drift_missing_binding_and_missing_policy() {
    let endpoint = self::endpoint();
    assert!(
        ContractRouter::<()>::new(registry(endpoint.contract()))
            .finish()
            .is_err()
    );
    let mut changed = endpoint.contract();
    changed.path = "/other".into();
    assert!(
        ContractRouter::<()>::new(registry(changed))
            .bind(endpoint, |_, note| async { Ok(note) })
            .is_err()
    );
    let endpoint = self::endpoint().with_access(AccessLevel::Authenticated);
    assert!(
        ContractRouter::<()>::new(registry(endpoint.contract()))
            .bind(endpoint, |_, note| async { Ok(note) })
            .is_err()
    );
}
#[derive(Clone)]
struct Deny;
#[allow(clippy::unused_async_trait_impl)]
impl EndpointPolicy<()> for Deny {
    type Context = ();
    async fn authorize(
        &self,
        _: &mut Parts,
        (): &(),
        _: &RouteContract,
    ) -> Result<(), EndpointError> {
        Err(EndpointError::Unauthorized)
    }
}
#[tokio::test]
async fn authorization_precedes_body_parsing_and_handler() {
    let endpoint = endpoint().with_access(AccessLevel::Authenticated);
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind_authorized(endpoint, Deny, |_, (), _| async {
            panic!("unauthorized handler executed");
            #[allow(unreachable_code)]
            Ok(Note {
                title: "unreachable".into(),
            })
        })
        .unwrap()
        .finish()
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/notes")
                .body(Body::from("malformed private data"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn finite_representation_owns_content_type_and_rejects_duplicate_binding() {
    let endpoint =
        Endpoint::<NoBody, HtmlBody>::new("Preview", "Preview", HttpMethod::Get, "/preview", 200)
            .unwrap();
    let published = endpoint.contract();
    assert!(
        published
            .responses
            .iter()
            .any(|response| response.status_code == 500)
    );
    let router = ContractRouter::<()>::new(registry(published))
        .bind(endpoint.clone(), |_, ()| async {
            Ok("<p>note</p>".to_owned())
        })
        .unwrap();
    let app = router.finish().unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/preview")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.headers()["content-type"],
        "text/html; charset=utf-8"
    );
    assert_eq!(
        &to_bytes(response.into_body(), 1000).await.unwrap()[..],
        b"<p>note</p>"
    );
    let router = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind(endpoint.clone(), |_, ()| async { Ok(String::new()) })
        .unwrap();
    assert!(
        router
            .bind(endpoint, |_, ()| async { Ok(String::new()) })
            .is_err()
    );
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Upload {
    file: forklaunch_core::contract::UploadFile,
}

#[tokio::test]
async fn finite_form_binary_and_multipart_bindings_use_typed_values() {
    let endpoint =
        Endpoint::<FormBody<Note>, BinaryBody>::new("form", "form", HttpMethod::Post, "/form", 200)
            .unwrap()
            .with_response_header("content-disposition", "attachment")
            .unwrap();
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind(endpoint, |_, note| async { Ok(note.title.into_bytes()) })
        .unwrap()
        .finish()
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/form")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("title=Hello+World"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "application/octet-stream"
    );
    assert_eq!(response.headers()["content-disposition"], "attachment");
    assert_eq!(
        &to_bytes(response.into_body(), 100).await.unwrap()[..],
        b"Hello World"
    );
    let endpoint = Endpoint::<MultipartBody<Upload>, BinaryBody>::new(
        "upload",
        "upload",
        HttpMethod::Post,
        "/upload",
        201,
    )
    .unwrap();
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind(endpoint, |_, upload| async move {
            assert_eq!(upload.files["file"].filename, "test.txt");
            Ok(upload.fields.file.0)
        })
        .unwrap()
        .finish()
        .unwrap();
    let part = "--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\nContent-Type: text/plain\r\n\r\nprivate content\r\n";
    for (body, expected) in [
        (format!("{part}--boundary--\r\n"), 201),
        (format!("{part}{part}--boundary--\r\n"), 400),
        ("malformed".into(), 400),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/upload")
                    .header("content-type", "multipart/form-data; boundary=boundary")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        if expected == 201 {
            assert_eq!(
                &to_bytes(response.into_body(), 100).await.unwrap()[..],
                b"private content"
            );
        }
    }
}

#[derive(forklaunch_core::contract::ForklaunchResponses)]
enum LookupResponse {
    #[response(status = 200)]
    Found(Note),
    #[response(status = 200, content_type = Text)]
    Pending(String),
    #[response(status = 200, content_type = Html)]
    Html(String),
    #[response(status = 204)]
    Empty,
}

#[tokio::test]
async fn response_enum_selects_status_media_and_validates_selected_schema() {
    use forklaunch_core::contract::endpoint::Responses;
    for (choice, status, content, expected) in [
        (0, 200, Some("application/json"), r#"{"title":"hello"}"#),
        (1, 200, Some("text/plain; charset=utf-8"), "pending"),
        (2, 204, None, ""),
        (3, 500, Some("application/json"), ""),
        (4, 200, Some("text/html; charset=utf-8"), "<p>hello</p>"),
    ] {
        let endpoint = Endpoint::<NoBody, Responses<LookupResponse>>::responses(
            "lookup",
            "lookup",
            HttpMethod::Get,
            "/lookup",
        )
        .unwrap();
        let contract = endpoint.contract();
        assert_eq!(
            contract
                .responses
                .iter()
                .filter(|r| r.status_code < 300)
                .count(),
            4
        );
        let app = ContractRouter::<()>::new(registry(contract))
            .bind(endpoint, move |_, ()| async move {
                Ok(match choice {
                    0 => LookupResponse::Found(Note {
                        title: "hello".into(),
                    }),
                    1 => LookupResponse::Pending("pending".into()),
                    2 => LookupResponse::Empty,
                    4 => LookupResponse::Html("<p>hello</p>".into()),
                    _ => LookupResponse::Found(Note { title: "x".into() }),
                })
            })
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/lookup")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .map(|v| v.to_str().unwrap()),
            content
        );
        let bytes = to_bytes(response.into_body(), 1000).await.unwrap();
        if status == 500 {
            assert!(!String::from_utf8_lossy(&bytes).contains("title"));
        } else {
            assert_eq!(&bytes[..], expected.as_bytes());
        }
    }
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchQuery {
    #[schemars(range(min = 1, max = 10))]
    limit: u32,
    tags: Vec<String>,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct SearchHeaders {
    #[serde(rename = "x-revision")]
    revision: u32,
}
#[tokio::test]
async fn typed_query_headers_bind_runtime_and_contract_schemas() {
    use forklaunch_core::contract::endpoint::Fields;
    let endpoint = Endpoint::<Fields<NoBody, SearchQuery, SearchHeaders>, TextBody>::new(
        "search",
        "search",
        HttpMethod::Get,
        "/search",
        200,
    )
    .unwrap();
    let route = endpoint.contract();
    assert_eq!(route.request.query.len(), 2);
    assert_eq!(route.request.headers[0].name, "x-revision");
    let schema: serde_json::Value = serde_json::from_str(
        route
            .request
            .query
            .iter()
            .find(|f| f.name == "limit")
            .unwrap()
            .schema
            .json_schema
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(schema["maximum"], 10.0);
    let app = ContractRouter::<()>::new(registry(route))
        .bind(endpoint, |_, request| async move {
            Ok(format!(
                "{}:{}:{}",
                request.query.limit,
                request.query.tags.join(","),
                request.headers.revision
            ))
        })
        .unwrap()
        .finish()
        .unwrap();
    for (uri, revision, status) in [
        ("/search?limit=2&tags=a&tags=b", "3", 200),
        ("/search?limit=2&tags=a", "3", 200),
        ("/search?limit=20&tags=a", "3", 400),
        ("/search?limit=2&limit=3&tags=a", "3", 400),
        ("/search?tags=a", "3", 400),
        ("/search?limit=2&tags=a", "private-invalid", 400),
        ("/search?limit=2&tags=a&unknown=bad", "3", 400),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("X-Revision", revision)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{uri}");
        if status == 200 {
            let bytes = to_bytes(response.into_body(), 1000).await.unwrap();
            assert!(String::from_utf8_lossy(&bytes).starts_with("2:a"));
        }
    }
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct NumericPath {
    #[schemars(range(min = 1, max = 99))]
    note_id: u32,
}
#[tokio::test]
async fn typed_path_fields_coerce_validate_and_match_route_placeholders() {
    use forklaunch_core::contract::endpoint::{Fields, NoFields};
    type Input = Fields<NoBody, NoFields, NoFields, NumericPath>;
    assert!(
        Endpoint::<Input, TextBody>::new("bad", "bad", HttpMethod::Get, "/notes/:other", 200)
            .is_err()
    );
    let endpoint =
        Endpoint::<Input, TextBody>::new("path", "path", HttpMethod::Get, "/notes/:note_id", 200)
            .unwrap();
    assert_eq!(endpoint.contract().request.params[0].name, "note_id");
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind(endpoint, |_, request| async move {
            Ok(request.path.note_id.to_string())
        })
        .unwrap()
        .finish()
        .unwrap();
    for (path, status) in [
        ("/notes/3", 200),
        ("/notes/0", 400),
        ("/notes/100", 400),
        ("/notes/private-value", 400),
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        if status == 200 {
            assert_eq!(
                &to_bytes(response.into_body(), 100).await.unwrap()[..],
                b"3"
            );
        }
    }
}

#[derive(forklaunch_core::contract::ForklaunchResponses)]
enum ExportResponse {
    #[response(status = 200, content_type = Text, media_type = Csv)]
    Csv(String),
    #[response(status = 200, content_type = Binary, media_type = Pdf)]
    Pdf(Vec<u8>),
}
#[tokio::test]
async fn typed_media_overrides_preserve_codecs_and_asset_metadata() {
    use forklaunch_core::contract::{MediaType, endpoint::Responses};
    for (pdf, expected) in [(false, "text/csv"), (true, "application/pdf")] {
        let endpoint = Endpoint::<TextBody, Responses<ExportResponse>>::responses(
            "export",
            "export",
            HttpMethod::Post,
            "/export",
        )
        .unwrap()
        .with_request_media(MediaType::Csv)
        .unwrap();
        let contract = endpoint.contract();
        assert_eq!(
            contract.request.body.unwrap().content_type.as_deref(),
            Some("text/csv")
        );
        let app = ContractRouter::<()>::new(registry(endpoint.contract()))
            .bind(endpoint, move |_, body| async move {
                Ok(if pdf {
                    ExportResponse::Pdf(body.into_bytes())
                } else {
                    ExportResponse::Csv(body)
                })
            })
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/export")
                    .header("content-type", "text/csv")
                    .body(Body::from("title,body"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["content-type"], expected);
        assert_eq!(
            &to_bytes(response.into_body(), 100).await.unwrap()[..],
            b"title,body"
        );
    }
    let endpoint =
        Endpoint::<NoBody, TextBody>::new("text", "text", HttpMethod::Get, "/text", 200).unwrap();
    assert!(
        endpoint
            .clone()
            .with_response_media(MediaType::Pdf)
            .is_err()
    );
    assert!(
        endpoint
            .clone()
            .with_response_media(MediaType::Custom {
                representation: forklaunch_core::contract::ContentType::Text,
                value: "text/csv\r\nX-Bad: yes"
            })
            .is_err()
    );
    let endpoint = endpoint.with_response_media(MediaType::Markdown).unwrap();
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind(endpoint, |_, ()| async { Ok("# hello".into()) })
        .unwrap()
        .finish()
        .unwrap();
    let response = app
        .oneshot(Request::builder().uri("/text").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "text/markdown");
}

#[tokio::test]
async fn optional_body_preserves_absence_and_validates_present_values() {
    let endpoint = Endpoint::<OptionalBody<JsonBody<Note>>, TextBody>::new(
        "optional",
        "optional",
        HttpMethod::Post,
        "/optional",
        200,
    )
    .unwrap();
    assert!(endpoint.contract().request.body_optional);
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind(endpoint, |_, body| async move {
            Ok(body.map_or_else(|| "absent".into(), |note| note.title))
        })
        .unwrap()
        .finish()
        .unwrap();
    for (body, content, status) in [
        ("", "", 200),
        (r#"{"title":"hello"}"#, "application/json", 200),
        (r#"{"title":"x"}"#, "application/json", 400),
        ("private", "text/plain", 415),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/optional")
                    .header("content-type", content)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        if body.is_empty() {
            assert_eq!(
                &to_bytes(response.into_body(), 100).await.unwrap()[..],
                b"absent"
            );
        }
    }
}

#[derive(serde::Serialize, schemars::JsonSchema)]
struct MissingNote {
    code: String,
}
#[derive(forklaunch_core::contract::ForklaunchResponses)]
enum FindResponse {
    #[response(status = 200)]
    Found(Note),
    #[response(status = 404)]
    Missing(MissingNote),
    #[response(status = 302)]
    Moved,
}
#[tokio::test]
async fn typed_errors_merge_framework_failures_and_redirects_retain_status() {
    use forklaunch_core::contract::endpoint::Responses;
    for (choice, status) in [(0, 200), (1, 404), (2, 302), (3, 404)] {
        let endpoint = Endpoint::<NoBody, Responses<FindResponse>>::responses(
            "find",
            "find",
            HttpMethod::Get,
            "/find",
        )
        .unwrap()
        .with_response_header_for(302, "", "location", "/next")
        .unwrap();
        let contract = endpoint.contract();
        let schema = contract
            .responses
            .iter()
            .find(|r| r.status_code == 404)
            .unwrap()
            .body
            .as_ref()
            .unwrap()
            .schema
            .json_schema
            .as_ref()
            .unwrap();
        let schema: serde_json::Value = serde_json::from_str(schema).unwrap();
        assert_eq!(schema["anyOf"].as_array().unwrap().len(), 2);
        let app = ContractRouter::<()>::new(registry(contract))
            .bind(endpoint, move |_, ()| async move {
                match choice {
                    0 => Ok(FindResponse::Found(Note {
                        title: "hello".into(),
                    })),
                    1 => Ok(FindResponse::Missing(MissingNote {
                        code: "note_missing".into(),
                    })),
                    2 => Ok(FindResponse::Moved),
                    _ => Err(EndpointError::NotFound),
                }
            })
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .oneshot(Request::builder().uri("/find").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        if status == 302 {
            assert_eq!(response.headers()["location"], "/next");
        }
        let bytes = to_bytes(response.into_body(), 1000).await.unwrap();
        if choice == 1 {
            assert!(String::from_utf8_lossy(&bytes).contains("note_missing"));
        }
        if choice == 3 {
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["error"],
                "not found"
            );
        }
    }
}

#[derive(Clone)]
struct SignedWirePolicy;
#[allow(clippy::unused_async_trait_impl)]
impl EndpointPolicy<()> for SignedWirePolicy {
    const CAPABILITIES: super::PolicyCapabilities = super::PolicyCapabilities::SignedInternal;
    type Context = bool;
    async fn authorize(
        &self,
        parts: &mut Parts,
        (): &(),
        _: &RouteContract,
    ) -> Result<bool, EndpointError> {
        if parts.headers.get("x-proof").and_then(|v| v.to_str().ok()) == Some("expected") {
            Ok(false)
        } else {
            Err(EndpointError::Unauthorized)
        }
    }
    async fn authorize_body(
        &self,
        _: bool,
        _: &mut Parts,
        bytes: &[u8],
        (): &(),
        _: &RouteContract,
    ) -> Result<bool, EndpointError> {
        if bytes == br#"{"title":"hello"}"# {
            Ok(true)
        } else {
            Err(EndpointError::Unauthorized)
        }
    }
}
#[tokio::test]
async fn signed_body_policy_sees_bounded_original_bytes_before_decoding() {
    let endpoint = endpoint().with_access(AccessLevel::Internal);
    let called = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = called.clone();
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind_authorized(endpoint, SignedWirePolicy, move |_, verified, note| {
            let observed = observed.clone();
            async move {
                assert!(verified);
                observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(note)
            }
        })
        .unwrap()
        .finish()
        .unwrap();
    for (body, status) in [
        (br#"{"title":"hello"}"#.to_vec(), 201),
        (br#"{ "title": "hello" }"#.to_vec(), 401),
        (b"private-malformed-json".to_vec(), 401),
        (vec![b'x'; 2 * 1024 * 1024 + 1], 413),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/notes")
                    .header("content-type", "application/json")
                    .header("x-proof", "expected")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    assert_eq!(called.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[derive(serde::Serialize, schemars::JsonSchema)]
struct OutputHeaders {
    #[serde(rename = "x-count")]
    #[schemars(range(min = 1))]
    count: u32,
    #[serde(rename = "x-next", skip_serializing_if = "Option::is_none")]
    next: Option<String>,
}
#[tokio::test]
async fn typed_dynamic_response_headers_are_declared_and_validated() {
    use forklaunch_core::contract::endpoint::ResponseWithHeaders;
    for count in [0, 2] {
        let typed = Endpoint::<NoBody, TextBody>::new(
            "Headers",
            "Headers",
            HttpMethod::Get,
            "/headers",
            200,
        )
        .unwrap()
        .with_typed_response_headers::<OutputHeaders>()
        .unwrap();
        assert_eq!(typed.contract().responses[0].headers.len(), 2);
        let app = ContractRouter::<()>::new(registry(typed.contract()))
            .bind(typed, move |_, ()| async move {
                Ok(ResponseWithHeaders {
                    body: "ok".into(),
                    headers: OutputHeaders { count, next: None },
                })
            })
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/headers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if count == 0 {
            assert_eq!(response.status(), 500);
        } else {
            assert_eq!(response.status(), 200);
            assert_eq!(response.headers()["x-count"], "2");
            assert!(!response.headers().contains_key("x-next"));
        }
    }
}

#[derive(forklaunch_core::contract::ForklaunchResponses)]
enum VendorReply {
    #[response(status = 200, media_type = "application/vnd.forklaunch.note+json")]
    Note(Note),
    #[response(status=200, content_type=Text, media_type="text/x-forklaunch-note")]
    Text(String),
}
#[tokio::test]
async fn response_enum_custom_mime_preserves_typed_codec_and_schema() {
    use forklaunch_core::contract::endpoint::Responses;
    for json in [true, false] {
        let typed = Endpoint::<NoBody, Responses<VendorReply>>::responses(
            "Vendor",
            "Vendor",
            HttpMethod::Get,
            "/vendor",
        )
        .unwrap();
        let app = ContractRouter::<()>::new(registry(typed.contract()))
            .bind(typed, move |_, ()| async move {
                Ok(if json {
                    VendorReply::Note(Note {
                        title: "typed".into(),
                    })
                } else {
                    VendorReply::Text("plain".into())
                })
            })
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/vendor")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers()["content-type"],
            if json {
                "application/vnd.forklaunch.note+json"
            } else {
                "text/x-forklaunch-note"
            }
        );
        assert_eq!(
            to_bytes(response.into_body(), 1024).await.unwrap(),
            if json {
                r#"{"title":"typed"}"#
            } else {
                "plain"
            }
        );
    }
}

#[forklaunch_core::contract::schema(unknown_fields = "deny")]
struct FormProfile {
    name: String,
    ages: Vec<u16>,
    enabled: bool,
}
#[forklaunch_core::contract::schema(unknown_fields = "deny")]
struct NestedForm {
    profile: FormProfile,
    labels: Vec<String>,
}
#[tokio::test]
async fn nested_urlencoded_forms_bind_arrays_and_enforce_limits_before_handler() {
    let endpoint = Endpoint::<FormBody<NestedForm>, JsonBody<NestedForm>>::new(
        "nested",
        "nested",
        HttpMethod::Post,
        "/nested",
        200,
    )
    .unwrap();
    let app = ContractRouter::<()>::new(registry(endpoint.contract()))
        .bind(endpoint, |_, form| async move { Ok(form) })
        .unwrap()
        .finish()
        .unwrap();
    for (body, media, expected) in [
        ("profile[name]=007&profile[ages][]=2&profile[ages][]=3&profile[enabled]=true&labels[]=a&labels[]=b".into(), "application/x-www-form-urlencoded", 200),
        ("profile[name]=007&profile[ages][]=70000&profile[enabled]=true&labels[]=a".into(), "application/x-www-form-urlencoded", 400),
        ("profile[name]=007&profile[ages][]=2&profile[enabled]=true&labels[]=a&extra=leak-canary".into(), "application/x-www-form-urlencoded", 400),
        ("a=1&".repeat(1001), "application/x-www-form-urlencoded", 413),
        ("a".repeat(102_401), "application/x-www-form-urlencoded", 413),
        (format!("a{}=leak-canary", "[b]".repeat(33)), "application/x-www-form-urlencoded", 400),
        ("a=1".into(), "application/x-www-form-urlencoded; charset=utf-16", 415),
    ] {
        let response = app.clone().oneshot(Request::builder().method("POST").uri("/nested").header("content-type",media).body(Body::from(body)).unwrap()).await.unwrap();
        assert_eq!(response.status().as_u16(), expected);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        if expected == 200 {
            assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(), serde_json::json!({"profile":{"name":"007","ages":[2,3],"enabled":true},"labels":["a","b"]}));
        } else { assert!(!String::from_utf8_lossy(&bytes).contains("leak-canary")); }
    }
}

#[test]
fn ordinary_authentication_cannot_mount_internal_or_protected_endpoints() {
    for access in [AccessLevel::Internal, AccessLevel::Protected] {
        let endpoint = endpoint().with_access(access);
        let result = ContractRouter::<()>::new(registry(endpoint.contract())).bind_authorized(
            endpoint,
            Deny,
            |_, (), note| async { Ok(note) },
        );
        assert!(result.is_err());
    }
    let route = endpoint()
        .contract()
        .with_auth(forklaunch_core::contract::AuthConfig::hmac());
    assert!(PolicyCapabilities::Authenticated.admit(&route).is_err());
    assert!(PolicyCapabilities::SignedInternal.admit(&route).is_ok());
    let route = route.with_access(AccessLevel::Protected);
    assert!(PolicyCapabilities::SignedInternal.admit(&route).is_err());
    assert!(
        PolicyCapabilities::AuthorizedSignedInternal
            .admit(&route)
            .is_ok()
    );
}
