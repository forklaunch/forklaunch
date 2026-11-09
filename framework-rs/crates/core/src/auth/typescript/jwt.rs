//! Opt-in TypeScript JWT credential callback semantics.
use super::authorization_token;
use crate::http::{ErrorKind, ErrorResponse};
use serde_json::Value;
use std::future::Future;

/// Await a trusted application-owned JWT verifier or resource decoder.
/// This function only parses the header and interprets callback results; the
/// callback must verify the credential, including all required signature/claim
/// checks. Never supply an unverified JWT payload decoder as authentication.
/// Validate returned sessions and bind their principal before authorizing them.
///
/// The first raw token is passed unchanged after exact prefix checks. TypeScript
/// falsy JSON results (null, false, zero, empty string) reject authentication;
/// arrays and objects, including empty ones, are preserved. Dropping the future
/// cancels polling the callback, without retrying it.
///
/// # Errors
/// Returns TypeScript 401 missing/format errors before invoking the callback.
/// Callback failures and falsy claims return a generic 403 without error details.
pub async fn verify_jwt_resource<'a, Fut>(
    header: Option<&'a str>,
    token_prefix: Option<&str>,
    verify: impl FnOnce(&'a str) -> Fut,
) -> Result<Value, ErrorResponse>
where
    Fut: Future<Output = Result<Value, ErrorResponse>>,
{
    let token = authorization_token(header, token_prefix.unwrap_or("Bearer"))?;
    let claims = verify(token).await.map_err(|_| rejected())?;
    let falsy = match &claims {
        Value::Null => true,
        Value::Bool(value) => !value,
        Value::Number(value) => value.as_f64() == Some(0.0),
        Value::String(value) => value.is_empty(),
        Value::Array(_) | Value::Object(_) => false,
    };
    if falsy {
        return Err(rejected());
    }
    Ok(claims)
}

fn rejected() -> ErrorResponse {
    ErrorResponse::new(ErrorKind::Forbidden, "Invalid Authorization token.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn callback_errors_are_redacted_and_falsy_claims_rejected() {
        for value in [Value::Null, json!(false), json!(0), json!(-0.0), json!("")] {
            let error = verify_jwt_resource(Some("Bearer opaque"), None, |_| async { Ok(value) })
                .await
                .unwrap_err();
            assert_eq!(error.message, "Invalid Authorization token.");
            assert_eq!(error.to_response().status_code, 403);
        }
        let error = verify_jwt_resource(Some("Bearer opaque"), None, |_| async {
            Err(ErrorResponse::new(
                ErrorKind::Internal,
                "private provider failure",
            ))
        })
        .await
        .unwrap_err();
        assert_eq!(error.message, "Invalid Authorization token.");
        assert_eq!(error.to_response().status_code, 403);
    }

    #[tokio::test]
    async fn prefix_checks_precede_verification_and_claims_are_preserved() {
        for header in [
            None,
            Some("Bearer"),
            Some("bearer token"),
            Some(" Bearer token"),
        ] {
            assert!(
                verify_jwt_resource(header, None, |_| async {
                    panic!("invalid header must not call verifier");
                })
                .await
                .is_err()
            );
        }
        for claims in [
            json!({}),
            json!([]),
            json!(true),
            json!(1),
            json!("verified"),
            json!({"sub":"verified", "team":"one"}),
        ] {
            let expected = claims.clone();
            let value = verify_jwt_resource(
                Some("Custom opaque ignored"),
                Some("Custom"),
                |token| async move {
                    assert_eq!(token, "opaque");
                    tokio::task::yield_now().await;
                    Ok(claims)
                },
            )
            .await
            .unwrap();
            assert_eq!(value, expected);
        }
    }
}
