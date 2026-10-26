//! Per-request HMAC signing against the framework verifier and a nonce store.
use axum::{
    Router,
    body::Bytes,
    http::{HeaderMap, Method, StatusCode, Uri},
};
use forklaunch_core::auth::request_hmac::{HmacAuthorization, SignedBody};
use forklaunch_universal_sdk::{HmacSigner, MultipartForm, SdkRequest, UniversalSdk};
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

const SECRET: &[u8] = b"universal-sdk-hmac-fixture-secret-32b";

type Seen = Arc<Mutex<(HashSet<String>, Vec<String>)>>;

// The same checks a generated `SignedInternal` policy performs: wire path, original
// bytes, a five-minute window and an atomic nonce claim after verification.
fn verifier(seen: Seen) -> Router {
    Router::new().fallback(
        move |method: Method, uri: Uri, headers: HeaderMap, bytes: Bytes| {
            let seen = Arc::clone(&seen);
            async move {
                let Some(header) = headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok())
                else {
                    return StatusCode::UNAUTHORIZED;
                };
                let Ok(credential) = HmacAuthorization::parse(header) else {
                    return StatusCode::UNAUTHORIZED;
                };
                let body = if bytes.is_empty() {
                    SignedBody::Absent
                } else {
                    SignedBody::Bytes(&bytes)
                };
                let key = if credential.key_id() == "other" {
                    b"other-key-fixture-secret".as_slice()
                } else {
                    SECRET
                };
                if !credential.verify_at(
                    key,
                    method.as_str(),
                    uri.path(),
                    body,
                    chrono::Utc::now().timestamp_millis(),
                    Duration::from_secs(300),
                ) {
                    return StatusCode::FORBIDDEN;
                }
                let mut seen = seen.lock().unwrap();
                seen.1.push(header.to_owned());
                if seen.0.insert(credential.nonce().to_owned()) {
                    StatusCode::OK
                } else {
                    StatusCode::CONFLICT
                }
            }
        },
    )
}

fn request(method: &'static str, url: String) -> SdkRequest {
    SdkRequest {
        method,
        url,
        query: BTreeMap::new(),
        headers: BTreeMap::new(),
        body: None,
        body_bytes: None,
        multipart: None,
        content_type: "application/json",
    }
}

#[tokio::test]
async fn signs_every_request_over_wire_path_and_bytes_and_replays_are_rejected() {
    let seen: Seen = Arc::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = verifier(Arc::clone(&seen));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = format!("http://{address}");
    let client = UniversalSdk::new(&base).with_hmac_secret(SECRET);

    // A JSON body, a query string (never signed) and a nested full path.
    let mut post = request("POST", format!("{base}/payment-link"));
    post.body = Some(r#"{"amount":12,"currency":"USD"}"#.into());
    post.query.insert("expand".into(), "true".into());
    // A caller-supplied credential is replaced by the per-request signature.
    post.headers
        .insert("Authorization".into(), "HMAC stale".into());
    for _ in 0..2 {
        let response = client.execute(post.clone()).await.unwrap();
        assert_eq!(response.status(), 200);
    }
    let response = client
        .execute(request("GET", format!("{base}/payment-link/abc%2Fdef")))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // A hook that rewrites the body runs before signing.
    let hooked = UniversalSdk::new(&base)
        .with_auth_hook(|input| input.body = Some("rewritten".into()))
        .with_hmac_secret(SECRET);
    let mut put = request("PUT", format!("{base}/jobs"));
    put.body = Some("original".into());
    assert_eq!(hooked.execute(put).await.unwrap().status(), 200);

    // Streaming multipart bodies are buffered and signed over the sent bytes.
    let multipart = UniversalSdk::<MultipartForm>::new(&base).with_hmac_secret(SECRET);
    let mut upload = SdkRequest::<MultipartForm> {
        method: "POST",
        url: format!("{base}/upload"),
        query: BTreeMap::new(),
        headers: BTreeMap::new(),
        body: None,
        body_bytes: None,
        multipart: Some(MultipartForm::new().text("title", "hello")),
        content_type: "multipart/form-data",
    };
    assert_eq!(multipart.execute(upload).await.unwrap().status(), 200);
    upload = SdkRequest::<MultipartForm> {
        method: "POST",
        url: format!("{base}/upload"),
        query: BTreeMap::new(),
        headers: BTreeMap::new(),
        body: None,
        body_bytes: None,
        multipart: None,
        content_type: "application/octet-stream",
    };
    upload.body_bytes = Some(vec![0, 159, 146, 150]);
    assert_eq!(multipart.execute(upload).await.unwrap().status(), 200);

    // Named keys and wrong secrets.
    let named = UniversalSdk::new(&base)
        .with_hmac_signer(HmacSigner::with_key_id("other", "other-key-fixture-secret"));
    assert_eq!(
        named
            .execute(request("DELETE", format!("{base}/plan/1")))
            .await
            .unwrap()
            .status(),
        200
    );
    let wrong = UniversalSdk::new(&base).with_hmac_secret("not-the-secret");
    assert_eq!(
        wrong
            .execute(request("GET", format!("{base}/plan")))
            .await
            .unwrap()
            .status(),
        403
    );

    // Every accepted call carried its own nonce; replaying one is refused.
    let captured = seen.lock().unwrap().1.clone();
    assert_eq!(captured.len(), 7);
    assert_eq!(
        captured.iter().collect::<HashSet<_>>().len(),
        captured.len()
    );
    let replay = reqwest::Client::new()
        .post(format!("{base}/payment-link"))
        .header("authorization", &captured[0])
        .body(r#"{"amount":12,"currency":"USD"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), 409);
    server.abort();
}

#[test]
fn signer_matches_the_typescript_payload_and_redacts_the_secret() {
    let signer = HmacSigner::new(SECRET);
    let header = signer
        .sign_at(
            "POST",
            "/payment-link",
            Some(br#"{"a":1}"#),
            "2026-10-22T00:00:00.000Z",
            "nonce",
        )
        .unwrap();
    let payload = "POST\n/payment-link\n{\"a\":1}\n2026-10-22T00:00:00.000Z\nnonce";
    assert_eq!(
        header,
        format!(
            "HMAC keyId=default ts=2026-10-22T00:00:00.000Z nonce=nonce signature={}",
            forklaunch_core::auth::hmac_sign(SECRET, payload.as_bytes())
        )
    );
    // Absent and empty bodies both use TypeScript's literal "undefined".
    for body in [None, Some(&b""[..])] {
        let header = signer
            .sign_at("GET", "/plan", body, "2026-10-22T00:00:00.000Z", "n")
            .unwrap();
        assert!(header.ends_with(&forklaunch_core::auth::hmac_sign(
            SECRET,
            b"GET\n/plan\nundefined2026-10-22T00:00:00.000Z\nn"
        )));
    }
    assert!(signer.sign("GET", "/a\nb", None).is_err());
    assert!(!format!("{signer:?}").contains("fixture"));
    assert_ne!(
        signer.sign("GET", "/plan", None).unwrap(),
        signer.sign("GET", "/plan", None).unwrap()
    );
}
