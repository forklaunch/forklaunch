use super::*;
use forklaunch_core::auth::oidc::OidcProvider;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    issuer: String,
    response: Arc<Mutex<String>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let metadata = json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/authorize"),"token_endpoint":format!("{issuer}/token"),"jwks_uri":format!("{issuer}/jwks"),"response_types_supported":["code"],"subject_types_supported":["public"],"id_token_signing_alg_values_supported":["RS256"]}).to_string();
        let keys = json!({"keys":[{"kty":"RSA","alg":"RS256","use":"sig","kid":"fixture","e":"AQAB","n":"kPszNF2ghKj80PvXDNA8pzL2TNEoJKp5g5eIbpPmjHJvyaItOcQOhKcvg5SkW7WVePRqGKIslE-NOg4qMS35oos1g5V9rOnsEunTRciygsCQkaL5WVBNerFLpuUBGypUOYd2W6TpKIsdENXBYO-SDckAYxEZUEdjZshi_AsgWQbfki7axKB67DgTonA0utX3JpG4h9PDVgj6erihubsUdM40Wuslh_j92H1Y3uGQBcvAnwlgvfd_t6WULNeeLAtng-ewemnqEbGSmuZiLPNU2TO9fnwa0C0Cm802BRyYdqDlw3JmLatuqLiO4Ft6PxQDSwlZa1uqy8Y0E3QHhGLp3Q"}]}).to_string();
        let response = Arc::new(Mutex::new(String::new()));
        let reply = response.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk[..count]);
                    let request = String::from_utf8_lossy(&bytes);
                    if let Some(end) = request.find("\r\n\r\n") {
                        let length = request[..end]
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|value| value.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8_lossy(&bytes);
                let body = if request.starts_with("GET /.well-known/") {
                    metadata.clone()
                } else if request.starts_with("GET /jwks") {
                    keys.clone()
                } else {
                    reply.lock().unwrap().clone()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        Self {
            issuer,
            response,
            task,
        }
    }
    fn token(&self, nonce: &str, subject: &str) {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({"iss":self.issuer,"aud":"local-client","sub":subject,"iat":now,"exp":now+300,"nonce":nonce,"email":"one@example.com","email_verified":true,"roles":["admin"]});
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some("fixture".into());
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(include_bytes!(
            "../../../jwt_sessions/test-key.pem"
        ))
        .unwrap();
        let token = jsonwebtoken::encode(&header, &claims, &key).unwrap();
        *self.response.lock().unwrap() =
            json!({"access_token":"provider-secret","token_type":"Bearer","id_token":token})
                .to_string();
    }
}

#[tokio::test]
#[ignore = "requires FORKLAUNCH_TEST_POSTGRES_URL"]
async fn postgres_oidc_exchange_issues_only_explicitly_bound_verified_sessions() {
    let fixture = Fixture::new().await;
    let provider = OidcProvider::discover(
        &fixture.issuer,
        "local-client".into(),
        None,
        "http://127.0.0.1/callback",
    )
    .await
    .unwrap();
    let (owner, pool, role) = super::super::super::tests::test_pools().await;
    let store = PostgresIamService::new(
        pool.clone(),
        TenantContext::new(format!("oidc-{}", uuid::Uuid::new_v4())).unwrap(),
    )
    .await
    .unwrap();
    let store = store.with_jwt_session_issuer(Arc::new(crate::jwt_sessions::tests::issuer()));
    store
        .create_user(User::new("one", "one@example.com"), "password".into())
        .await
        .unwrap();
    for (subject, wrong_nonce, bound) in [
        ("subject", false, false),
        ("subject", true, true),
        ("subject", false, true),
    ] {
        if bound {
            store
                .bind_provider_identity(&fixture.issuer, subject, "one")
                .await
                .unwrap();
        }
        let pending = provider.begin(&[]).unwrap();
        let state = pending.state().to_owned();
        let nonce = pending
            .authorization_url()
            .query_pairs()
            .find(|(name, _)| name == "nonce")
            .unwrap()
            .1
            .into_owned();
        fixture.token(if wrong_nonce { "attacker" } else { &nonce }, subject);
        let result = store
            .authenticate_oidc(&provider, pending, &state, "code")
            .await;
        if bound && !wrong_nonce {
            let session = result.unwrap();
            assert_eq!(session.user_id, "one");
            let claims = crate::jwt_sessions::tests::verify(&session.id);
            assert_eq!(claims["sub"], "one");
            assert_eq!(claims["roles"], json!([]));
            assert!(store.validate_session(&session.id).await.unwrap().is_some());
            assert!(
                store
                    .get_user("one")
                    .await
                    .unwrap()
                    .unwrap()
                    .roles
                    .is_empty()
            );
            store.revoke_session(&session.id).await.unwrap();
        } else {
            assert!(result.is_err());
        }
    }
    store.delete_user("one").await.unwrap();
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP OWNED BY {role}")))
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {role}")))
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

#[cfg(feature = "browser-sessions")]
#[tokio::test]
#[ignore = "requires FORKLAUNCH_TEST_POSTGRES_URL"]
#[allow(clippy::too_many_lines)] // One durable-state lifecycle covers restart, claim races and cleanup.
async fn postgres_oidc_pending_is_encrypted_browser_bound_and_consumed_once() {
    use crate::browser_sessions::PostgresCookieStore;
    use forklaunch_core::persistence::encryption::FieldEncryptor;
    use tower_sessions::{
        SessionStore,
        cookie::time::{Duration, OffsetDateTime},
        session::{Id, Record},
    };
    let fixture = Fixture::new().await;
    let provider = OidcProvider::discover(
        &fixture.issuer,
        "local-client".into(),
        None,
        "http://127.0.0.1/callback",
    )
    .await
    .unwrap();
    let (owner, pool, role) = super::super::super::tests::test_pools().await;
    let tenant = TenantContext::new(format!("pending-{}", uuid::Uuid::new_v4())).unwrap();
    let encryptor = Arc::new(FieldEncryptor::new("pending-fixture-only").unwrap());
    let store =
        PostgresCookieStore::new(pool.clone(), tenant.clone(), "browser", encryptor.clone())
            .await
            .unwrap();
    let mut record = Record {
        id: Id::default(),
        data: std::collections::HashMap::new(),
        expiry_date: OffsetDateTime::now_utc() + Duration::minutes(30),
    };
    store.create(&mut record).await.unwrap();
    let binding = record.id.to_string();
    let pending = provider.begin(&[]).unwrap();
    let state = pending.state().to_owned();
    let nonce = pending
        .authorization_url()
        .query_pairs()
        .find(|(name, _)| name == "nonce")
        .unwrap()
        .1
        .into_owned();
    fixture.token(&nonce, "subject");
    store.save_oidc_pending(&binding, pending).await.unwrap();
    let encrypted: String =
        sqlx::query_scalar("SELECT encrypted_pending FROM fl_iam_oidc_pending WHERE tenant_id=$1")
            .bind(tenant.id())
            .fetch_one(&owner)
            .await
            .unwrap();
    assert!(!encrypted.contains(&state));
    assert!(!encrypted.contains(&nonce));
    assert!(!encrypted.contains("verifier"));
    assert!(
        store
            .consume_oidc_pending(&Id::default().to_string(), &state)
            .await
            .is_err()
    );
    assert!(store.consume_oidc_pending(&binding, "wrong").await.is_err());
    let other = PostgresCookieStore::new(pool.clone(), tenant.clone(), "other", encryptor.clone())
        .await
        .unwrap();
    assert!(other.consume_oidc_pending(&binding, &state).await.is_err());
    let other = PostgresCookieStore::new(
        pool.clone(),
        TenantContext::new(format!("other-{}", uuid::Uuid::new_v4())).unwrap(),
        "browser",
        encryptor.clone(),
    )
    .await
    .unwrap();
    assert!(other.consume_oidc_pending(&binding, &state).await.is_err());
    let set_role = format!("SET ROLE {role}");
    let other_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |connection, _| {
            let set_role = set_role.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(set_role))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    let restarted =
        PostgresCookieStore::new(other_pool.clone(), tenant.clone(), "browser", encryptor)
            .await
            .unwrap();
    let (first, second) = tokio::join!(
        store.consume_oidc_pending(&binding, &state),
        restarted.consume_oidc_pending(&binding, &state)
    );
    assert_ne!(first.is_ok(), second.is_ok());
    let restored = first.or(second).unwrap();
    assert!(provider.exchange(restored, &state, "code").await.is_ok());
    assert!(store.consume_oidc_pending(&binding, &state).await.is_err());
    let pending = provider.begin(&[]).unwrap();
    let state = pending.state().to_owned();
    store.save_oidc_pending(&binding, pending).await.unwrap();
    sqlx::query("UPDATE fl_iam_oidc_pending SET expires_at=0 WHERE tenant_id=$1")
        .bind(tenant.id())
        .execute(&owner)
        .await
        .unwrap();
    assert!(store.consume_oidc_pending(&binding, &state).await.is_err());
    let pending = provider.begin(&[]).unwrap();
    let state = pending.state().to_owned();
    store.save_oidc_pending(&binding, pending).await.unwrap();
    store.delete(&record.id).await.unwrap();
    assert!(store.consume_oidc_pending(&binding, &state).await.is_err());
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_oidc_pending WHERE tenant_id=$1")
            .bind(tenant.id())
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(count, 0);
    other_pool.close().await;
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP OWNED BY {role}")))
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {role}")))
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}
