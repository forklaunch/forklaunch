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
