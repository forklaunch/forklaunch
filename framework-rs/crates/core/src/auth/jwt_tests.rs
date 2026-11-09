// Test-only signing material, never used by runtime code.
pub(crate) const RSA_PRIVATE: &[u8] = br"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC4TpQNSQrBuQ/X
YX116SRxKGEgyYhTV1gkCepv8B1wN/Ht/oEGMpa/UIaKzPhmWNFqCIDp+Ttz3xhT
1nha+gOY35r/A9QC9oXWJg0eOVw5dCbY30zADgQu6uwl5uHszRs1/pz7oXyCTdKD
yeVTbNMZqsiRYCJIM+6RAe03AjzXjguydbamDlc7ctPrQ4AaPRWiPMVYEYodlbpu
io0vZQGAthbv9cD3RmvIxDU+LhTZP6mEPR1PjAqcbXgHO7AV773Otpwoal04byTa
eDJTAMBwrN/+8m/hMOYJSbYz1HNBwJMC1BNsDDjhFilN9ueVDL/NpzonKSWbKcAS
Ucr3D7vJAgMBAAECggEAAXuWWtYB2WWjAD1XTE3MHoHv6Bre9yaufZm70ls47Whx
0JOZ16IsDs13y6GTBQXFZ4g6e0dnSEsc7727uaWlxkLURSVzfPOdR4gGVn4pJQgv
cNK5FBl4xKTcsgBdxHEw/Rn3itm1ffngzfEfoHBxcVUrqGnA1NpgGrgMf8MLmMNX
C385upRVxYk9bM7su7soh5g7p8/H6UfgoE7q937LclAMGFvgohKhFgQjU71gTrG1
D9+w4XwTLJJsVJV0iiMYBlwpEnhkuN4WlCZUc+K2IgH7oUE1JC/f9NW6AZ93CgHS
04Y7D8Pz2XxmqFeu5JG7+3QKFg+uB4AK1tHAfNhuNQKBgQDaYUJrFV81aUtOEyeL
rN+dPEVt06mXZS6enapjOxCcrPzTO30zlLpx4bjp9MxcXwwsc3eE4mWRnEr8JySV
HBYVHi+2flrtRNVG8Dzj8vNrAqSmRrDwJEgfGSOxkzNtkbR1VHHQQ9/uKQ6ZFeGw
XNVYDQQ02fmiACntWPMk44adXQKBgQDYDqv2kb7c7+NS1tjPFiRqEs/FOiZK0MhC
7tYTqW4Iw/JcJhmiKnYMxJViaLi6/gSCfQuhzyEN/UuuPx324UQI2TjhOyGsRjwQ
DXpBSE1qcb0kmtacElssUFFO3dIas2QzXIsBfWDLqLwdJlEQvVBDnDX/jC0p77Dh
PGu7B1DFXQKBgGa0My51gK9388imTYTtKsWiIO0EDNB87639bLL6hleJK+aUJc/q
dL02cWbmkG4o7WrCa0V9zFLs9H8eUaAm82O/LKVhS5YnNz8DoT97dWuwdIsW2AQ4
rAqkIhtTpW40hnBf5J8ULO6vNIRevCPbpSKiDIZmtXaxKQbNPJzuvNKpAoGBAKL5
NjfQqnMHtKa5D2PuJNuqgvUXrujCYj0y4WKFbE3KWJev2agcdV2eL7zCl43drFMt
tceBcUKXLS4ryBOAyowOJrymmPjVJdtJ/00dF+BSGUVyUq35nrJnh12BL0oaxVuE
1An0pD4bcoJec8m3s8K8I3M3Lp4ygm7z36N9miy1AoGAb1Q9sQUsZPbULnWfGQ8a
NO5+EYLDDhF2jWzQYme2KeIt3BAEmJadX3/5Mkrjyxwv+C2eG2Y7a6v2hSVMDxQP
Obb8pdBbfeTuGaKECBi4hI9dSAxkh0yPd/sooOntngFiGuswjbIJiCoH4AlNP6bo
cSSdh47MrwO+b+Nr0uuT8z0=
-----END PRIVATE KEY-----
";
pub(crate) fn rsa_jwk(kid: &str) -> jsonwebtoken::jwk::Jwk {
    serde_json::from_value(serde_json::json!({"kty":"RSA","n":"uE6UDUkKwbkP12F9dekkcShhIMmIU1dYJAnqb_AdcDfx7f6BBjKWv1CGisz4ZljRagiA6fk7c98YU9Z4WvoDmN-a_wPUAvaF1iYNHjlcOXQm2N9MwA4ELursJebh7M0bNf6c-6F8gk3Sg8nlU2zTGarIkWAiSDPukQHtNwI8144LsnW2pg5XO3LT60OAGj0VojzFWBGKHZW6boqNL2UBgLYW7_XA90ZryMQ1Pi4U2T-phD0dT4wKnG14BzuwFe-9zracKGpdOG8k2ngyUwDAcKzf_vJv4TDmCUm2M9RzQcCTAtQTbAw44RYpTfbnlQy_zac6JyklmynAElHK9w-7yQ","e":"AQAB","alg":"RS256","use":"sig","kid":kid})).unwrap()
}

use super::*;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};

fn token(kid: &str, changes: &Value) -> String {
    let mut claims = json!({"sub":"user-1", "exp":4_102_444_800_u64, "iss":"issuer", "aud":"service", "tenant":"tenant-1"});
    claims
        .as_object_mut()
        .unwrap()
        .extend(changes.as_object().unwrap().clone());
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(RSA_PRIVATE).unwrap(),
    )
    .unwrap()
}

fn policy() -> Validation {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&["issuer"]);
    validation.set_audience(&["service"]);
    validation.validate_nbf = true;
    validation
}

#[test]
fn jwk_enforces_signature_algorithm_key_usage_and_claim_policy() {
    let signed = token("key-1", &json!({}));
    let key = rsa_jwk("key-1");
    let claims: Value = verify_jwk(&signed, &key, &policy()).unwrap();
    assert_eq!(claims["tenant"], "tenant-1");
    for change in [
        json!({"iss":"other"}),
        json!({"aud":"other"}),
        json!({"exp":1}),
        json!({"nbf":4_102_444_800_u64}),
    ] {
        assert!(verify_jwk::<Value>(&token("key-1", &change), &key, &policy()).is_err());
    }
    assert!(verify_jwk::<Value>(&signed, &rsa_jwk("other"), &policy()).is_err());
    assert!(verify_jwk::<Value>(&signed, &key, &Validation::new(Algorithm::HS256)).is_err());
    for metadata in [
        json!({"use":"enc"}),
        json!({"key_ops":["sign"]}),
        json!({"alg":"RS512"}),
    ] {
        let mut changed = serde_json::to_value(&key).unwrap();
        changed
            .as_object_mut()
            .unwrap()
            .extend(metadata.as_object().unwrap().clone());
        assert!(
            verify_jwk::<Value>(
                &signed,
                &serde_json::from_value(changed).unwrap(),
                &policy()
            )
            .is_err()
        );
    }
    let mut pieces: Vec<_> = signed.split('.').map(str::to_owned).collect();
    pieces[1] = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        b"{\"sub\":\"attacker\",\"exp\":4102444800}",
    );
    assert!(verify_jwk::<Value>(&pieces.join("."), &key, &policy()).is_err());
}

#[test]
fn jwk_accepts_an_allowed_algorithm_when_policy_spans_key_families() {
    let signed = token("key-1", &json!({}));
    let mut validation = policy();
    validation.algorithms = vec![Algorithm::RS256, Algorithm::ES256, Algorithm::EdDSA];
    let claims: Value = verify_jwk(&signed, &rsa_jwk("key-1"), &validation).unwrap();
    assert_eq!(claims["tenant"], "tenant-1");
    assert_eq!(
        validation.algorithms.len(),
        3,
        "caller policy must remain unchanged"
    );
    validation.algorithms = vec![Algorithm::ES256, Algorithm::EdDSA];
    assert!(verify_jwk::<Value>(&signed, &rsa_jwk("key-1"), &validation).is_err());
}

#[test]
fn jwks_rejects_unknown_and_ambiguous_keys() {
    let signed = token("key-1", &json!({}));
    for keys in [
        vec![],
        vec![rsa_jwk("other")],
        vec![rsa_jwk("key-1"), rsa_jwk("key-1")],
    ] {
        assert!(verify_jwks::<Value>(&signed, &JwkSet { keys }, &policy()).is_err());
    }
}

#[cfg(feature = "remote-jwks")]
mod remote_tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // A real HTTP endpoint exposes mutable keys and request counts. This checks
    // transport/cache behavior rather than mocking the verifier itself.
    async fn endpoint() -> (
        String,
        Arc<std::sync::Mutex<(u16, String, String)>>,
        Arc<AtomicUsize>,
        tokio::task::JoinHandle<()>,
    ) {
        let state = Arc::new(std::sync::Mutex::new((
            200,
            serde_json::to_string(&JwkSet {
                keys: vec![rsa_jwk("key-1")],
            })
            .unwrap(),
            "public, max-age=3600".to_owned(),
        )));
        let calls = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/keys", listener.local_addr().unwrap());
        let shared = state.clone();
        let count = calls.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                let read = stream.read(&mut request).await.unwrap();
                if read == 0 {
                    continue;
                }
                count.fetch_add(1, Ordering::Relaxed);
                let (status, body, cache_control) = shared.lock().unwrap().clone();
                let response = format!(
                    "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nCache-Control: {cache_control}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        (url, state, calls, task)
    }

    #[tokio::test]
    async fn remote_cache_deduplicates_concurrent_fetches_and_bounds_unknown_kids() {
        let (url, _, calls, server) = endpoint().await;
        let verifier = RemoteJwks::new(RemoteJwksConfig::new(url)).unwrap();
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let verifier = verifier.clone();
            tasks.push(tokio::spawn(async move {
                verifier
                    .verify::<Value>(&token("key-1", &json!({})), &policy())
                    .await
                    .unwrap()
            }));
        }
        for task in tasks {
            assert_eq!(task.await.unwrap()["sub"], "user-1");
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        for _ in 0..5 {
            assert!(
                verifier
                    .verify::<Value>(&token("unknown", &json!({})), &policy())
                    .await
                    .is_err()
            );
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        server.abort();
    }

    #[tokio::test]
    async fn remote_rotation_expiry_failures_and_response_limits() {
        let (url, state, calls, server) = endpoint().await;
        let mut config = RemoteJwksConfig::new(url.clone());
        config.refresh_cooldown = Duration::ZERO;
        let verifier = RemoteJwks::new(config).unwrap();
        verifier
            .verify::<Value>(&token("key-1", &json!({})), &policy())
            .await
            .unwrap();
        state.lock().unwrap().1 = serde_json::to_string(&JwkSet {
            keys: vec![rsa_jwk("key-2")],
        })
        .unwrap();
        verifier
            .verify::<Value>(&token("key-2", &json!({})), &policy())
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert!(
            verifier
                .verify::<Value>(&token("key-1", &json!({})), &policy())
                .await
                .is_err()
        );
        let mut config = RemoteJwksConfig::new(url.clone());
        config.max_ttl = Duration::from_millis(1);
        config.refresh_cooldown = Duration::ZERO;
        let expires = RemoteJwks::new(config).unwrap();
        expires
            .verify::<Value>(&token("key-2", &json!({})), &policy())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        state.lock().unwrap().0 = 503;
        assert!(
            expires
                .verify::<Value>(&token("key-2", &json!({})), &policy())
                .await
                .is_err()
        );
        state.lock().unwrap().0 = 200;
        state.lock().unwrap().1 = "not JSON".to_owned();
        assert!(
            expires
                .verify::<Value>(&token("key-2", &json!({})), &policy())
                .await
                .is_err()
        );
        let mut config = RemoteJwksConfig::new(url);
        config.max_response_bytes = 4;
        assert!(
            RemoteJwks::new(config)
                .unwrap()
                .verify::<Value>(&token("key-2", &json!({})), &policy())
                .await
                .is_err()
        );
        server.abort();
    }

    #[test]
    fn remote_endpoint_rejects_other_schemes_and_credentials() {
        for url in [
            "ftp://example.com/keys",
            "https://user:password@example.com/keys",
            "https://example.com/keys#fragment",
        ] {
            assert!(RemoteJwks::new(RemoteJwksConfig::new(url)).is_err());
        }
    }

    #[test]
    fn remote_endpoint_accepts_internal_http_but_keeps_other_checks() {
        assert!(RemoteJwks::new(RemoteJwksConfig::new("http://iam:8002/api/auth/jwks")).is_ok());
    }
}

#[test]
fn correctly_signed_tokens_with_unsupported_critical_headers_are_rejected() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let secret = b"critical-header-test-key";
    let jwk = serde_json::from_value(
        json!({"kty":"oct", "k":URL_SAFE_NO_PAD.encode(secret), "alg":"HS256"}),
    )
    .unwrap();
    for header in [
        json!({"alg":"HS256","crit":["extension"],"extension":true}),
        json!({"alg":"HS256","b64":false}),
    ] {
        let message = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap()),
            URL_SAFE_NO_PAD.encode(br#"{"sub":"user","exp":4102444800}"#)
        );
        let signature = jsonwebtoken::crypto::sign(
            message.as_bytes(),
            &EncodingKey::from_secret(secret),
            Algorithm::HS256,
        )
        .unwrap();
        let token = format!("{message}.{signature}");
        assert!(crate::auth::verify_jwt_hs256(&token, secret).is_err());
        assert!(verify_jwk::<Value>(&token, &jwk, &Validation::new(Algorithm::HS256)).is_err());
    }
}
