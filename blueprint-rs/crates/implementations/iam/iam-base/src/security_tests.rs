use super::*;
#[test]
fn authentication_issues_unique_short_lived_sessions_and_rejects_disabled_users() {
    let credentials = || Credentials {
        email: "test@example.com".to_owned(),
        password: "secret".to_owned(),
    };
    let service = InMemoryIamService::new(vec![User::new("user_1", "test@example.com")])
        .with_password("test@example.com", "secret")
        .unwrap();
    let first = service.authenticate(credentials()).unwrap();
    let second = service.authenticate(credentials()).unwrap();
    assert_ne!(first.id, second.id);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(first.expires_at_unix_seconds > now);
    assert!(first.expires_at_unix_seconds <= now + 3600);
    let mut disabled = User::new("user_1", "test@example.com");
    disabled.disabled = true;
    let service = InMemoryIamService::new(vec![disabled])
        .with_password("test@example.com", "secret")
        .unwrap();
    assert!(service.authenticate(credentials()).is_err());
}

#[test]
fn out_of_range_pagination_does_not_panic() {
    let service = InMemoryIamService::default();
    let request = PageRequest {
        limit: 10,
        cursor: Some(usize::MAX.to_string()),
    };
    assert!(
        service
            .list_users(request.clone())
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        service
            .list_organizations(request)
            .unwrap()
            .items
            .is_empty()
    );
}
