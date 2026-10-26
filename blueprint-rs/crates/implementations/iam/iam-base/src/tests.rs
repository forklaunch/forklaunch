use super::*;

#[test]
fn in_memory_iam_authenticates_and_lists_users() {
    let service = InMemoryIamService::new(vec![User::new("user_1", "test@example.com")])
        .with_password("test@example.com", "secret")
        .expect("password hash")
        .with_permission("user_1", Permission::new("accounts", "read"));

    let session = service
        .authenticate(Credentials {
            email: "test@example.com".to_owned(),
            password: "secret".to_owned(),
        })
        .expect("credentials should be valid");
    let page = service
        .list_users(PageRequest::first_page(10))
        .expect("list should succeed");

    assert_eq!(session.user_id, "user_1");
    assert_eq!(page.items.len(), 1);
    assert_eq!(
        service
            .permissions_for_user("user_1")
            .expect("permissions should load")
            .len(),
        1
    );
}

#[test]
fn in_memory_iam_resolves_organizations_and_roles() {
    use forklaunch_iam_interface::{Organization, Role};

    let service = InMemoryIamService::new(vec![User::new("user_1", "test@example.com")])
        .with_organization(Organization::new("org_1", "Acme", "acme"))
        .with_role(
            "user_1",
            Role::new("admin").with_permission(Permission::new("accounts", "read")),
        );

    assert_eq!(
        service
            .get_organization("org_1")
            .expect("org lookup should succeed")
            .map(|organization| organization.slug),
        Some("acme".to_owned())
    );
    assert_eq!(
        service
            .list_organizations(PageRequest::first_page(10))
            .expect("org list should succeed")
            .items
            .len(),
        1
    );
    assert_eq!(
        service
            .roles_for_user("user_1")
            .expect("roles should load")
            .len(),
        1
    );
}
