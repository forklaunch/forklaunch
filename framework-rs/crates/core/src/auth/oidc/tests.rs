use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct ProviderFixture {
    issuer: String,
    reply: Arc<Mutex<Value>>,
    metadata: Arc<Mutex<Value>>,
    keys: Arc<Mutex<Value>>,
    key_requests: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<String>>>,
    count: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for ProviderFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl ProviderFixture {
    async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let base = issuer.clone();
        let metadata = Arc::new(Mutex::new(json!({
            "issuer":base,"authorization_endpoint":format!("{base}/authorize"),
            "token_endpoint":format!("{base}/token"),"jwks_uri":format!("{base}/jwks"),
            "response_types_supported":["code"],"subject_types_supported":["public"],
            "id_token_signing_alg_values_supported":["RS256"]
        })));
        let document = metadata.clone();
        let reply = Arc::new(Mutex::new(json!({})));
        let response = reply.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let received = requests.clone();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let keys = Arc::new(Mutex::new(
            json!({"keys":[crate::auth::jwt::tests::rsa_jwk("fixture")]}),
        ));
        let served_keys = keys.clone();
        let key_requests = Arc::new(Mutex::new(Vec::new()));
        let received_keys = key_requests.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let size = socket.read(&mut chunk).await.unwrap();
                    if size == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk[..size]);
                    let text = String::from_utf8_lossy(&bytes);
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|v| v.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let text = String::from_utf8(bytes).unwrap();
                let path = text.split_whitespace().nth(1).unwrap();
                if path == "/chunked" {
                    let body = " ".repeat(1024 * 1024 + 1);
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(reply.as_bytes()).await;
                    continue;
                }
                let (status, extra, body) = match path {
                    "/.well-known/openid-configuration" => {
                        ("200 OK", "", document.lock().unwrap().to_string())
                    }
                    "/jwks" => {
                        received_keys.lock().unwrap().push(text);
                        ("200 OK", "", served_keys.lock().unwrap().to_string())
                    }
                    "/token" => {
                        calls.fetch_add(1, Ordering::SeqCst);
                        received.lock().unwrap().push(text);
                        ("200 OK", "", response.lock().unwrap().to_string())
                    }
                    "/oversized" => ("200 OK", "", " ".repeat(1024 * 1024 + 1)),
                    "/redirect" => (
                        "302 Found",
                        "Location: http://example.invalid/private\r\n",
                        String::new(),
                    ),
                    _ => ("404 Not Found", "", "{}".into()),
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
            }
        });
        Self {
            issuer,
            reply,
            metadata,
            keys,
            key_requests,
            requests,
            count,
            task,
        }
    }
    async fn client(&self) -> OidcProvider {
        OidcProvider::discover(
            &self.issuer,
            "client".into(),
            Some("private-client-secret".into()),
            "http://127.0.0.1/callback",
        )
        .await
        .unwrap()
    }
    fn respond(&self, pending: &PendingAuthorization, changes: &Value) {
        self.respond_with_nonce(pending.nonce.secret(), changes);
    }
    fn respond_with_nonce(&self, nonce: &str, changes: &Value) {
        self.respond_with_key(
            nonce,
            changes,
            "fixture",
            crate::auth::jwt::tests::RSA_PRIVATE,
        );
    }
    fn respond_with_key(&self, nonce: &str, changes: &Value, kid: &str, key: &[u8]) {
        let now = chrono::Utc::now().timestamp();
        let digest = Sha256::digest(b"private-access-token");
        let mut claims = json!({"sub":"subject", "iss":self.issuer,"aud":"client", "exp":now+300,"iat":now,"nonce":nonce,"at_hash":URL_SAFE_NO_PAD.encode(&digest[..16]),"email":"person@example.test","email_verified":true});
        claims
            .as_object_mut()
            .unwrap()
            .extend(changes.as_object().unwrap().clone());
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.into());
        let token = encode(&header, &claims, &EncodingKey::from_rsa_pem(key).unwrap()).unwrap();
        *self.reply.lock().unwrap() = json!({"access_token":"private-access-token","refresh_token":"private-refresh-token","token_type":"Bearer","expires_in":300,"id_token":token});
    }
}

#[tokio::test]
async fn discovery_pkce_exchange_and_verified_claims_use_bounded_transport() {
    let fixture = ProviderFixture::new().await;
    let client = fixture.client().await;
    let pending = client.begin(&["openid".into(), "email".into()]).unwrap();
    let query: std::collections::HashMap<_, _> = pending
        .authorization_url()
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(query["response_type"], "code");
    assert_eq!(query["scope"], "openid email");
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(
        query["code_challenge"],
        URL_SAFE_NO_PAD.encode(Sha256::digest(pending.verifier.secret().as_bytes()))
    );
    assert_eq!(query["nonce"], *pending.nonce.secret());
    assert_ne!(pending.state(), client.begin(&[]).unwrap().state());
    fixture.respond(&pending, &json!({}));
    let state = pending.state().to_owned();
    let verifier = pending.verifier.secret().clone();
    assert!(!format!("{pending:?} {client:?}").contains(&state));
    let login = client
        .exchange(pending, &state, "code with + unicode 水")
        .await
        .unwrap();
    assert_eq!(login.claims.subject().as_str(), "subject");
    assert_eq!(login.claims.email_verified(), Some(true));
    assert_eq!(login.tokens.access_token().secret(), "private-access-token");
    assert!(!format!("{login:?}").contains("private"));
    let request = fixture.requests.lock().unwrap()[0].clone();
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: basic ")
    );
    let form: std::collections::HashMap<_, _> =
        oauth2::url::form_urlencoded::parse(request.split_once("\r\n\r\n").unwrap().1.as_bytes())
            .into_owned()
            .collect();
    assert_eq!(form["code_verifier"], verifier);
    assert_eq!(form["code"], "code with + unicode 水");
    assert_eq!(form["redirect_uri"], "http://127.0.0.1/callback");
    assert!(!form.contains_key("client_secret"));
}

#[tokio::test]
async fn state_expiry_and_configuration_mismatch_fail_before_exchange() {
    let fixture = ProviderFixture::new().await;
    let client = fixture.client().await;
    for mutation in [
        "state",
        "expired",
        "issuer",
        "client",
        "redirect",
        "empty-code",
    ] {
        let mut pending = client.begin(&[]).unwrap();
        let mut state = pending.state().to_owned();
        let mut code = "code";
        match mutation {
            "state" => state.push('x'),
            "expired" => pending.created -= Duration::from_secs(601),
            "issuer" => pending.issuer.push('x'),
            "client" => pending.client_id.push('x'),
            "redirect" => pending.redirect.push('x'),
            "empty-code" => code = "",
            _ => unreachable!(),
        }
        assert!(
            client.exchange(pending, &state, code).await.is_err(),
            "{mutation}"
        );
        assert_eq!(fixture.count.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn public_clients_send_client_id_without_a_client_secret() {
    let fixture = ProviderFixture::new().await;
    fixture.metadata.lock().unwrap()["token_endpoint_auth_methods_supported"] = json!(["none"]);
    let client = OidcProvider::discover(
        &fixture.issuer,
        "client".into(),
        None,
        "http://127.0.0.1/callback",
    )
    .await
    .unwrap();
    let pending = client.begin(&[]).unwrap();
    fixture.respond(&pending, &json!({}));
    let state = pending.state().to_owned();
    let login = client
        .exchange(pending, &state, "public-code")
        .await
        .unwrap();
    assert_eq!(login.claims.subject().as_str(), "subject");
    let request = fixture.requests.lock().unwrap()[0].clone();
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    let form: std::collections::HashMap<_, _> =
        oauth2::url::form_urlencoded::parse(request.split_once("\r\n\r\n").unwrap().1.as_bytes())
            .into_owned()
            .collect();
    assert_eq!(form["client_id"], "client");
    assert!(!form.contains_key("client_secret"));
}

#[tokio::test]
async fn invalid_id_tokens_and_access_token_substitution_fail_closed() {
    let fixture = ProviderFixture::new().await;
    let client = fixture.client().await;
    for changes in [
        json!({"iss":"https://wrong.example"}),
        json!({"aud":"other"}),
        json!({"aud":["client","other"]}),
        json!({"exp":1}),
        json!({"iat":chrono::Utc::now().timestamp()+3600}),
        json!({"nonce":"wrong"}),
        json!({"nonce":null}),
        json!({"at_hash":"wrong"}),
        json!({"azp":"wrong"}),
        json!({"sub":""}),
    ] {
        let pending = client.begin(&[]).unwrap();
        fixture.respond(&pending, &changes);
        let state = pending.state().to_owned();
        let error = client.exchange(pending, &state, "code").await.unwrap_err();
        assert_eq!(error.message(), "OIDC authentication failed", "{changes}");
    }
    for mutation in [
        "missing-token",
        "bad-signature",
        "substituted-access",
        "provider-error",
    ] {
        let pending = client.begin(&[]).unwrap();
        fixture.respond(&pending, &json!({}));
        {
            let mut response = fixture.reply.lock().unwrap();
            match mutation {
                "missing-token" => {
                    response.as_object_mut().unwrap().remove("id_token");
                }
                "bad-signature" => {
                    let token = response["id_token"].as_str().unwrap();
                    let (signed, _) = token.rsplit_once('.').unwrap();
                    response["id_token"] = json!(format!("{signed}.AAAA"));
                }
                "substituted-access" => response["access_token"] = json!("substituted"),
                "provider-error" => {
                    *response = json!({"error":"invalid_grant","error_description":"private provider detail"});
                }
                _ => unreachable!(),
            }
        }
        let state = pending.state().to_owned();
        assert_eq!(
            client
                .exchange(pending, &state, "code")
                .await
                .unwrap_err()
                .message(),
            "OIDC authentication failed"
        );
    }
}

#[tokio::test]
async fn transport_rejects_insecure_endpoints_bounds_bodies_and_does_not_follow_redirects() {
    use oauth2::AsyncHttpClient as _;
    for url in [
        "http://example.com",
        "file:///tmp/test",
        "https://user:secret@example.com",
        "https://example.com/#fragment",
    ] {
        assert!(transport::endpoint(url).is_err());
    }
    let fixture = ProviderFixture::new().await;
    let transport = transport::Transport::new().unwrap();
    let request = |path| {
        oauth2::http::Request::get(format!("{}{path}", fixture.issuer))
            .body(Vec::new())
            .unwrap()
    };
    assert!(transport.call(request("/oversized")).await.is_err());
    assert!(transport.call(request("/chunked")).await.is_err());
    assert_eq!(
        transport.call(request("/redirect")).await.unwrap().status(),
        302
    );
}

#[tokio::test]
async fn invalid_discovery_metadata_and_scope_configuration_fail_closed() {
    let fixture = ProviderFixture::new().await;
    let original = fixture.metadata.lock().unwrap().clone();
    for (field, value) in [
        ("issuer", json!("https://other.example")),
        ("authorization_endpoint", json!("http://other.example")),
        ("token_endpoint", json!("http://other.example")),
        ("token_endpoint", Value::Null),
        ("jwks_uri", json!(format!("{}/oversized", fixture.issuer))),
        ("jwks_uri", json!(format!("{}/redirect", fixture.issuer))),
        ("id_token_signing_alg_values_supported", json!(["HS256"])),
        ("response_types_supported", json!(["id_token"])),
        (
            "token_endpoint_auth_methods_supported",
            json!(["client_secret_post"]),
        ),
    ] {
        let mut document = original.clone();
        document[field] = value;
        *fixture.metadata.lock().unwrap() = document;
        assert!(
            OidcProvider::discover(
                &fixture.issuer,
                "client".into(),
                Some("secret".into()),
                "http://127.0.0.1/callback"
            )
            .await
            .is_err(),
            "{field}"
        );
    }
    *fixture.metadata.lock().unwrap() = original;
    let client = fixture.client().await;
    for scopes in [
        vec![String::new()],
        vec!["two scopes".into()],
        vec!["email\n".into()],
        vec!["x".repeat(257)],
        vec!["email".into(); 65],
    ] {
        assert!(client.begin(&scopes).is_err());
    }
    assert_eq!(fixture.count.load(Ordering::SeqCst), 0);
}

async fn initial_login(fixture: &ProviderFixture, client: &OidcProvider) -> OidcLogin {
    let pending = client.begin(&[]).unwrap();
    fixture.respond(
        &pending,
        &json!({"auth_time":chrono::Utc::now().timestamp()-90}),
    );
    let state = pending.state().to_owned();
    client
        .exchange(pending, &state, "initial-code")
        .await
        .unwrap()
}

#[tokio::test]
async fn refresh_preserves_original_identity_and_handles_optional_tokens_and_rotation() {
    let fixture = ProviderFixture::new().await;
    let client = fixture.client().await;
    let login = initial_login(&fixture, &client).await;
    let original = login
        .claims
        .clone()
        .set_expiration(chrono::Utc::now() - chrono::Duration::seconds(1));
    let refresh_token = login.tokens.refresh_token().unwrap();
    for rotated in [None, Some("rotated-refresh-token")] {
        let mut response = json!({"access_token":"fresh-access", "token_type":"Bearer"});
        if let Some(rotated) = rotated {
            response["refresh_token"] = json!(rotated);
        }
        *fixture.reply.lock().unwrap() = response;
        let key_requests = fixture.key_requests.lock().unwrap().len();
        let refreshed = client.refresh(refresh_token, &original).await.unwrap();
        assert!(refreshed.claims.is_none());
        assert_eq!(fixture.key_requests.lock().unwrap().len(), key_requests);
        assert_eq!(
            refreshed.tokens.refresh_token().unwrap().secret(),
            rotated.unwrap_or(refresh_token.secret())
        );
        assert!(!format!("{refreshed:?}").contains("fresh-access"));
    }
    for include_nonce in [false, true] {
        let mut changes = json!({"auth_time":original.auth_time().unwrap().timestamp()});
        if !include_nonce {
            changes["nonce"] = Value::Null;
        }
        fixture.respond_with_nonce(original.nonce().unwrap().secret(), &changes);
        let refreshed = client.refresh(refresh_token, &original).await.unwrap();
        let claims = refreshed.claims.unwrap();
        assert_eq!(claims.subject(), original.subject());
        assert_eq!(claims.auth_time(), original.auth_time());
        assert_eq!(claims.nonce().is_some(), include_nonce);
    }
    let requests = fixture.requests.lock().unwrap();
    for request in requests.iter().skip(1) {
        let form: std::collections::HashMap<_, _> = oauth2::url::form_urlencoded::parse(
            request.split_once("\r\n\r\n").unwrap().1.as_bytes(),
        )
        .into_owned()
        .collect();
        assert_eq!(form["grant_type"], "refresh_token");
        assert_eq!(form["refresh_token"], *refresh_token.secret());
        assert!(!form.contains_key("code_verifier"));
    }
}

#[tokio::test]
async fn refreshed_id_tokens_cannot_change_the_original_login() {
    let fixture = ProviderFixture::new().await;
    let client = fixture.client().await;
    let login = initial_login(&fixture, &client).await;
    let original = &login.claims;
    let refresh_token = login.tokens.refresh_token().unwrap();
    for changes in [
        json!({"sub":"other"}),
        json!({"iss":"https://other.example"}),
        json!({"aud":"other"}),
        json!({"aud":["client","client"]}),
        json!({"nonce":"other"}),
        json!({"auth_time":1}),
        json!({"iat":1}),
        json!({"exp":1}),
        json!({"at_hash":"wrong"}),
    ] {
        fixture.respond_with_nonce(original.nonce().unwrap().secret(), &changes);
        assert_eq!(
            client
                .refresh(refresh_token, original)
                .await
                .unwrap_err()
                .message(),
            "OIDC authentication failed",
            "{changes}"
        );
    }
    for response in [
        json!({"access_token":"", "token_type":"Bearer"}),
        json!({"access_token":"token", "token_type":"Bearer", "refresh_token":""}),
        json!({"access_token":"token", "token_type":"MAC"}),
        json!({"error":"invalid_grant","error_description":"private refresh detail"}),
    ] {
        *fixture.reply.lock().unwrap() = response;
        assert_eq!(
            client
                .refresh(refresh_token, original)
                .await
                .unwrap_err()
                .message(),
            "OIDC authentication failed"
        );
    }
}

#[tokio::test]
async fn refresh_rejects_another_provider_or_client_before_network_access() {
    let fixture = ProviderFixture::new().await;
    let client = fixture.client().await;
    let login = initial_login(&fixture, &client).await;
    let refresh_token = login.tokens.refresh_token().unwrap();
    for original in [
        login
            .claims
            .clone()
            .set_issuer(IssuerUrl::new("https://other.example".into()).unwrap()),
        login
            .claims
            .clone()
            .set_audiences(vec![openidconnect::Audience::new("other".into())]),
        login.claims.clone().set_audiences(vec![]),
    ] {
        assert!(client.refresh(refresh_token, &original).await.is_err());
    }
    assert_eq!(fixture.count.load(Ordering::SeqCst), 1);
}

// Generated test-only RSA key; never used by production authentication.
const ROTATION_PRIVATE: &[u8] = include_bytes!("rotation-test.pem");
fn rotation_jwk() -> Value {
    json!({"kty":"RSA", "use":"sig", "alg":"RS256", "kid":"rotated", "e":"AQAB", "n":"kPszNF2ghKj80PvXDNA8pzL2TNEoJKp5g5eIbpPmjHJvyaItOcQOhKcvg5SkW7WVePRqGKIslE-NOg4qMS35oos1g5V9rOnsEunTRciygsCQkaL5WVBNerFLpuUBGypUOYd2W6TpKIsdENXBYO-SDckAYxEZUEdjZshi_AsgWQbfki7axKB67DgTonA0utX3JpG4h9PDVgj6erihubsUdM40Wuslh_j92H1Y3uGQBcvAnwlgvfd_t6WULNeeLAtng-ewemnqEbGSmuZiLPNU2TO9fnwa0C0Cm802BRyYdqDlw3JmLatuqLiO4Ft6PxQDSwlZa1uqy8Y0E3QHhGLp3Q"})
}

#[tokio::test]
async fn signing_key_rotation_is_verified_without_recreating_the_provider() {
    let fixture = ProviderFixture::new().await;
    let client = fixture.client().await;
    assert_eq!(fixture.key_requests.lock().unwrap().len(), 1);
    let pending = client.begin(&[]).unwrap();
    let state = pending.state().to_owned();
    fixture.respond(&pending, &json!({}));
    let original = client
        .exchange(pending, &state, "first-code")
        .await
        .unwrap();
    *fixture.keys.lock().unwrap() = json!({"keys":[rotation_jwk()]});
    let pending = client.begin(&[]).unwrap();
    let state = pending.state().to_owned();
    fixture.respond_with_key(
        pending.nonce.secret(),
        &json!({}),
        "rotated",
        ROTATION_PRIVATE,
    );
    let rotated = client
        .exchange(pending, &state, "second-code")
        .await
        .unwrap();
    assert_eq!(rotated.claims.subject(), original.claims.subject());
    fixture.respond_with_key(
        original.claims.nonce().unwrap().secret(),
        &json!({}),
        "rotated",
        ROTATION_PRIVATE,
    );
    let refreshed = client
        .refresh(original.tokens.refresh_token().unwrap(), &original.claims)
        .await
        .unwrap();
    assert_eq!(
        refreshed.claims.unwrap().subject(),
        original.claims.subject()
    );
    assert_eq!(fixture.count.load(Ordering::SeqCst), 3);
    let requests = fixture.key_requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    for request in requests.iter() {
        assert!(request.starts_with("GET /jwks "));
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        assert!(!request.contains("private-"));
    }
}

#[tokio::test]
async fn unavailable_or_revoked_keys_never_fall_back_to_discovery_keys() {
    for keys in [
        json!({"keys":[]}),
        json!({"keys":[rotation_jwk()]}),
        json!({"provider_error":"private-detail"}),
        json!({"keys":[], "padding":" ".repeat(1024 * 1024)}),
    ] {
        let fixture = ProviderFixture::new().await;
        let client = fixture.client().await;
        *fixture.keys.lock().unwrap() = keys;
        let pending = client.begin(&[]).unwrap();
        let state = pending.state().to_owned();
        fixture.respond(&pending, &json!({}));
        let error = client.exchange(pending, &state, "code").await.unwrap_err();
        assert_eq!(
            error.to_string(),
            "oidc_authentication_failed: OIDC authentication failed"
        );
        assert_eq!(fixture.count.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.key_requests.lock().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn durable_pending_preserves_nonce_pkce_binding_and_absolute_expiry() {
    let fixture = ProviderFixture::new().await;
    let client = fixture.client().await;
    let pending = client.begin(&[]).unwrap();
    let state = pending.state().to_owned();
    let expiry = pending.expires_at_unix_seconds();
    fixture.respond(&pending, &json!({}));
    let secret = pending.into_storage_secret().unwrap();
    let restored = PendingAuthorization::from_storage_secret(&secret).unwrap();
    assert_eq!(restored.expires_at_unix_seconds(), expiry);
    assert_eq!(restored.state(), state);
    assert!(client.exchange(restored, &state, "code").await.is_ok());
    for expiry in [
        0,
        chrono::Utc::now().timestamp(),
        chrono::Utc::now().timestamp() + 601,
        i64::MAX,
        i64::MIN,
    ] {
        let mut invalid: Value = serde_json::from_str(&secret).unwrap();
        invalid["expires_at"] = json!(expiry);
        assert!(PendingAuthorization::from_storage_secret(&invalid.to_string()).is_err());
    }
    assert!(PendingAuthorization::from_storage_secret(&"x".repeat(32769)).is_err());
    let mut invalid: Value = serde_json::from_str(&secret).unwrap();
    invalid["verifier"] = json!("short");
    assert!(PendingAuthorization::from_storage_secret(&invalid.to_string()).is_err());
}
