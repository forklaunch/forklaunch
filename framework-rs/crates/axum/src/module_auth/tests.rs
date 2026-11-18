use super::*;
use forklaunch_core::auth::jwt::RemoteJwksConfig;
const SECRET: &[u8] = b"module-test-secret";
fn jwks() -> RemoteJwks {
    RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap()
}
#[tokio::test]
async fn role_resolver_retains_application_claims_for_organization_scoping() {
    let config = HmacJwksAuth::new(SECRET.to_vec(), jwks(), Validation::new(Algorithm::RS256))
        .unwrap()
        .with_role_resolver(|claims| async move {
            assert_eq!(claims["activeOrganizationId"], "org-cac");
            assert_eq!(claims["sub"], "user-cac");
            Ok(vec!["system".to_owned()])
        });
    let roles =
        (config.roles)(serde_json::json!({"sub":"user-cac","activeOrganizationId":"org-cac"}))
            .await
            .unwrap();
    assert_eq!(roles, ["system"]);
}

#[test]
fn jwt_policy_rejects_symmetric_algorithms_and_disabled_expiry() {
    assert!(HmacJwksAuth::new(SECRET.to_vec(), jwks(), Validation::new(Algorithm::HS256)).is_err());
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = false;
    assert!(HmacJwksAuth::new(SECRET.to_vec(), jwks(), validation).is_err());
}

mod replay {
    use super::*;
    use forklaunch_core::auth::request_hmac::generate_hmac_auth_header;
    use forklaunch_core::contract::{HmacAuth, HttpMethod};
    use forklaunch_core::http::RequestContext;
    use std::sync::Mutex;

    const BODY: &str = r#"{"amount":100}"#;

    fn route() -> RouteContract {
        RouteContract::new("Charge", "Charge", HttpMethod::Post, "/charge")
            .with_auth(AuthConfig::Hmac(HmacAuth::new()))
    }
    fn auth() -> HmacJwksAuth {
        HmacJwksAuth::new(SECRET.to_vec(), jwks(), Validation::new(Algorithm::RS256)).unwrap()
    }
    fn timestamp(offset_seconds: i64) -> String {
        (chrono::Utc::now() + chrono::Duration::seconds(offset_seconds))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }
    fn sign(secret: &[u8], timestamp: &str, nonce: &str) -> String {
        generate_hmac_auth_header(
            secret,
            "default",
            "POST",
            "/charge",
            SignedBody::Text(BODY),
            timestamp,
            nonce,
        )
        .unwrap()
    }
    async fn send(auth: &HmacJwksAuth, authorization: &str, body: &str) -> bool {
        let request =
            FrameworkRequest::new(HttpMethod::Post, "/charge", RequestContext::new("replay"))
                .with_header("authorization", authorization)
                .with_body(Body::Text(body.into()));
        auth.authenticate(request, route()).await.is_ok()
    }

    #[tokio::test]
    async fn fresh_nonce_is_accepted_and_a_replayed_nonce_is_rejected() {
        let auth = auth();
        let first = sign(SECRET, &timestamp(0), "nonce-a");
        assert!(send(&auth, &first, BODY).await);
        // The identical request again: a replay.
        assert!(!send(&auth, &first, BODY).await);
        // The same nonce under a new timestamp is still a reuse.
        assert!(!send(&auth, &sign(SECRET, &timestamp(-1), "nonce-a"), BODY).await);
        // A retry that signs again with a fresh nonce passes.
        assert!(send(&auth, &sign(SECRET, &timestamp(0), "nonce-b"), BODY).await);
    }

    #[tokio::test]
    async fn stale_and_future_timestamps_are_rejected() {
        let auth = auth();
        assert!(!send(&auth, &sign(SECRET, &timestamp(-360), "stale"), BODY).await);
        assert!(!send(&auth, &sign(SECRET, &timestamp(360), "future"), BODY).await);
        assert!(send(&auth, &sign(SECRET, &timestamp(-240), "inside"), BODY).await);
    }

    #[tokio::test]
    async fn a_rejected_request_does_not_use_up_the_nonce() {
        let auth = auth();
        let genuine = sign(SECRET, &timestamp(0), "nonce-c");
        // Tampered body, wrong secret and stale timestamp, all with that nonce.
        assert!(!send(&auth, &genuine, r#"{"amount":999999}"#).await);
        assert!(!send(&auth, &sign(b"guessed", &timestamp(0), "nonce-c"), BODY).await);
        assert!(!send(&auth, &sign(SECRET, &timestamp(-3600), "nonce-c"), BODY).await);
        assert!(send(&auth, &genuine, BODY).await);
    }

    #[tokio::test]
    async fn shared_store_rejects_a_replay_sent_to_another_copy() {
        // Two copies of the service share one store, as they would share Redis.
        let claimed = Arc::new(Mutex::new(Vec::<(String, i64)>::new()));
        let copy = || {
            let claimed = Arc::clone(&claimed);
            auth().with_nonce_store(move |key, expiry| {
                let mut claimed = claimed.lock().unwrap();
                let first = !claimed.iter().any(|(seen, _)| *seen == key);
                if first {
                    claimed.push((key, expiry));
                }
                std::future::ready(Ok::<_, ()>(first))
            })
        };
        let (one, two) = (copy(), copy());
        let request = sign(SECRET, &timestamp(0), "nonce-d");
        let before = chrono::Utc::now().timestamp_millis();
        assert!(send(&one, &request, BODY).await);
        assert!(!send(&two, &request, BODY).await);
        let claimed = claimed.lock().unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].0, "7:defaultnonce-d");
        // Remembered until the signature can no longer pass the freshness check.
        assert!((claimed[0].1 - before - 300_000).abs() < 5_000);
    }

    #[tokio::test]
    async fn an_unavailable_nonce_store_fails_closed_and_is_not_asked_about_forgeries() {
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = Arc::clone(&asked);
        let auth = auth().with_nonce_store(move |_, _| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Err::<bool, _>("store unavailable"))
        });
        assert!(!send(&auth, &sign(b"guessed", &timestamp(0), "nonce-e"), BODY).await);
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(!send(&auth, &sign(SECRET, &timestamp(0), "nonce-e"), BODY).await);
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
