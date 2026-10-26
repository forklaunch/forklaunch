//! Opt-in compatibility with TypeScript Basic login and custom resource decoders.
use super::authorization_token;
use crate::auth::Principal;
use crate::http::{ErrorKind, ErrorResponse};
use std::future::Future;

/// Verify Basic credentials using the repository TypeScript parsing rules and a
/// trusted synchronous login callback. No principal is returned before login
/// succeeds. Passwords and decoded inputs are never included in errors.
///
/// This deliberately accepts Node's permissive base64 decoding and uses only the
/// first two colon-delimited fields. Use [`crate::auth::parse_basic`] for the native
/// strict decoder and passwords containing colons. This function is never enabled
/// by default. For trusted async resource decoding use [`verify_basic_resource`];
/// applications remain responsible for session validation.
///
/// # Errors
/// Returns the TypeScript 401 format/missing or 403 login error. The caller owns
/// the HTTP representation and any subsequent session authorization.
pub fn verify_basic(
    header: Option<&str>,
    token_prefix: Option<&str>,
    login: impl FnOnce(&str, &str) -> bool,
) -> Result<Principal, ErrorResponse> {
    let token = authorization_token(header, token_prefix.unwrap_or("Basic"))?;
    let bytes = decode_node_base64(token);
    let decoded = String::from_utf8_lossy(&bytes);
    let mut credentials = decoded.split(':');
    let username = credentials
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid_format)?;
    let password = credentials
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid_format)?;
    if !login(username, password) {
        return Err(ErrorResponse::new(
            ErrorKind::Forbidden,
            "Invalid Authorization login.",
        ));
    }
    Ok(Principal::new(username))
}

/// Verify a Basic token through a trusted application-owned async decoder.
/// The decoder receives the first token after the prefix unchanged, including an
/// empty token; it takes precedence over base64 parsing and synchronous login.
/// Null claims reject authentication. All other returned JSON values are preserved,
/// matching TypeScript; validate the session and bind its principal before use.
/// Never pass an unverified JSON/base64 decoder as credential verification.
/// Dropping this future cancels polling the decoder; it never retries it.
///
/// # Errors
/// Returns missing/format errors before invoking the decoder, the TypeScript 403
/// for null claims, or the decoder's explicitly selected public error unchanged.
pub async fn verify_basic_resource<'a, Fut>(
    header: Option<&'a str>,
    token_prefix: Option<&str>,
    decode: impl FnOnce(&'a str) -> Fut,
) -> Result<serde_json::Value, ErrorResponse>
where
    Fut: Future<Output = Result<serde_json::Value, ErrorResponse>>,
{
    let token = authorization_token(header, token_prefix.unwrap_or("Basic"))?;
    let claims = decode(token).await?;
    if claims.is_null() {
        return Err(ErrorResponse::new(
            ErrorKind::Forbidden,
            "Invalid Authorization token.",
        ));
    }
    Ok(claims)
}

fn invalid_format() -> ErrorResponse {
    ErrorResponse::new(
        ErrorKind::Unauthorized,
        "Invalid Authorization token format.",
    )
}

// Node consumes each UTF-16 code unit's low byte, accepts both alphabets, skips
// non-alphabet bytes, stops at padding and discards incomplete trailing bits.
fn decode_node_base64(input: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len().saturating_mul(3) / 4);
    let mut bits = 0_u32;
    let mut count = 0_u32;
    for unit in input.encode_utf16() {
        let byte = (unit & 0xff) as u8;
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            output.push(((bits >> count) & 0xff) as u8);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{verify_basic, verify_basic_resource};
    use crate::http::{ErrorKind, ErrorResponse};
    use serde_json::json;

    #[tokio::test]
    async fn async_decoder_owns_verification_and_preserves_claims() {
        for claims in [
            json!({"sub":"verified", "roles":["admin"]}),
            json!({}),
            json!(false),
            json!(0),
            json!(""),
            json!([]),
        ] {
            let result =
                verify_basic_resource(Some("Custom opaque ignored"), Some("Custom"), |token| {
                    let claims = claims.clone();
                    async move {
                        assert_eq!(token, "opaque");
                        tokio::task::yield_now().await;
                        Ok(claims)
                    }
                })
                .await
                .unwrap();
            assert_eq!(result, claims);
        }
        let error = verify_basic_resource(Some("Basic "), None, |token| async move {
            assert_eq!(token, "");
            Ok(serde_json::Value::Null)
        })
        .await
        .unwrap_err();
        assert_eq!(error.message, "Invalid Authorization token.");
        for header in [
            None,
            Some("Basic"),
            Some("basic opaque"),
            Some(" Basic opaque"),
        ] {
            assert!(
                verify_basic_resource(header, None, |_| async {
                    panic!("invalid prefix must not invoke decoder");
                })
                .await
                .is_err()
            );
        }
        let error = verify_basic_resource(Some("Basic opaque"), None, |_| async {
            Err(ErrorResponse::new(
                ErrorKind::Forbidden,
                "safe decoder denial",
            ))
        })
        .await
        .unwrap_err();
        assert_eq!(error.message, "safe decoder denial");
    }

    #[tokio::test]
    async fn cancelling_verification_drops_the_decoder_without_retry() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct Guard(Arc<AtomicUsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let calls = AtomicUsize::new(0);
        let decode_drops = drops.clone();
        let mut verification = Box::pin(verify_basic_resource(
            Some("Basic opaque"),
            None,
            |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                let _guard = Guard(decode_drops);
                std::future::pending().await
            },
        ));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(verification.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(verification);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn only_verified_credentials_create_a_principal() {
        let mut called = false;
        let result = verify_basic(
            Some("Basic dXNlcjpwYXNzOmV4dHJh"),
            None,
            |username, password| {
                called = true;
                assert_eq!((username, password), ("user", "pass"));
                false
            },
        );
        assert!(called);
        assert_eq!(result.unwrap_err().message, "Invalid Authorization login.");
        assert!(
            verify_basic(Some("basic dXNlcjpwYXNz"), None, |_, _| panic!(
                "invalid header must not call login"
            ))
            .is_err()
        );
        assert_eq!(
            crate::auth::parse_basic("Basic dXNlcjpwYXNzOmV4dHJh"),
            Some(("user".into(), "pass:extra".into()))
        );
        assert!(crate::auth::parse_basic("Basic dX!NlcjpwYXNz").is_none());
    }
}
