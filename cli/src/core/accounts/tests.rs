use tempfile::TempDir;

use super::*;

fn jwt(email: &str, org: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    format!(
        "{}.{}.sig",
        URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#),
        URL_SAFE_NO_PAD.encode(format!(
            r#"{{"email":"{email}","organizationId":"{org}","exp":1789701966}}"#
        ))
    )
}

fn token_file(email: &str, org: &str, refresh: &str) -> String {
    format!(
        "access_token = \"{}\"\nrefresh_token = \"{refresh}\"\nexpires_at = 1789701966\n",
        jwt(email, org)
    )
}

/// A machine as it looks before the keyring: one unsaved login.
fn legacy_home() -> TempDir {
    let home = TempDir::new().unwrap();
    fs::write(
        legacy_account_link(home.path()),
        token_file("first@example.com", "org-1", "session-1"),
    )
    .unwrap();
    home
}

fn add(dir: &Path, name: &str) {
    write_private_file(
        &account_path(dir, name),
        &token_file(&format!("{name}@example.com"), name, "flk_key"),
    )
    .unwrap();
}

/// A machine with a keyring: `first` saved and the default, plus `names`.
fn keyring(names: &[&str]) -> TempDir {
    let home = legacy_home();
    save(home.path(), "first", None, false).unwrap();
    for name in names {
        add(home.path(), name);
    }
    home
}

/// Who an older CLI, which reads only `~/.forklaunch/token`, would be.
fn what_an_older_cli_reads(dir: &Path) -> Option<String> {
    identity(&legacy_account_link(dir)).email
}

fn named(dir: &Path, name: &str) -> Selection {
    resolve(dir, Some(name), None, None).unwrap()
}

#[test]
fn an_untouched_machine_keeps_using_its_one_login() {
    let home = legacy_home();
    let selection = resolve(home.path(), None, None, None).unwrap();
    assert_eq!(selection.name, None);
    assert_eq!(selection.source, AccountSource::Default);
    assert_eq!(selection.token_path, legacy_account_link(home.path()));
    // Nothing to hand off: the login is already where an older CLI looks.
    assert!(handoff(home.path(), &selection).unwrap().is_none());
    assert!(!index_path(home.path()).exists());
}

#[test]
fn saving_the_default_login_keeps_it_the_default() {
    let home = legacy_home();
    let dir = home.path();
    save(dir, "first", None, false).unwrap();

    assert_eq!(default_profile_name(dir).as_deref(), Some("first"));
    let selection = resolve(dir, None, None, None).unwrap();
    assert_eq!(selection.name.as_deref(), Some("first"));
    assert_eq!(selection.token_path, account_path(dir, "first"));
    assert_eq!(
        what_an_older_cli_reads(dir).as_deref(),
        Some("first@example.com")
    );
    assert!(save(dir, "again", None, false).is_err());
}

#[test]
fn flag_beats_env_beats_binding_beats_default() {
    let home = keyring(&["flagged", "from-env", "bound"]);
    let dir = home.path();
    bind(dir, "app-1", Some("shop"), "bound").unwrap();
    let app = Some(("app-1", Some("shop")));

    let pick = |flag, env, app| resolve(dir, flag, env, app).unwrap();
    assert_eq!(
        pick(Some("flagged"), Some("from-env"), app).name.as_deref(),
        Some("flagged")
    );
    assert_eq!(
        pick(None, Some("from-env"), app).name.as_deref(),
        Some("from-env")
    );
    let bound = pick(None, None, app);
    assert_eq!(bound.name.as_deref(), Some("bound"));
    assert_eq!(bound.source.key(), "app-binding");
    assert_eq!(pick(None, None, None).name.as_deref(), Some("first"));
    // An application nobody bound falls through to the default.
    assert_eq!(
        pick(None, None, Some(("app-2", None))).name.as_deref(),
        Some("first")
    );
}

#[test]
fn a_named_account_with_no_login_still_resolves_so_login_can_create_it() {
    let home = TempDir::new().unwrap();
    let selection = named(home.path(), "new");
    assert_eq!(selection.token_path, account_path(home.path(), "new"));
    assert!(!selection.token_path.exists());
}

/// Binding ahead of the login is the safe order: the application can
/// then only fail for want of that login, never run as the default.
#[test]
fn an_application_can_be_bound_before_its_account_has_a_login() {
    let home = keyring(&[]);
    let dir = home.path();
    assert!(!bind(dir, "app-1", Some("shop"), "later").unwrap());

    let selection = resolve(dir, None, None, Some(("app-1", None))).unwrap();
    assert_eq!(selection.name.as_deref(), Some("later"));
    assert!(!selection.token_path.exists());
    assert!(missing_login_message(&selection).contains("later"));

    let listed = list(dir).unwrap();
    let later = listed
        .iter()
        .find(|a| a.name.as_deref() == Some("later"))
        .unwrap();
    assert!(!later.logged_in);
    assert_eq!(later.applications[0].name.as_deref(), Some("shop"));
}

#[test]
fn switching_the_default_does_not_lose_an_unsaved_login() {
    let home = legacy_home();
    let dir = home.path();
    add(dir, "other");

    let err = set_default(dir, "other", false).unwrap_err().to_string();
    assert!(err.contains("account save"), "{err}");
    assert_eq!(default_profile_name(dir), None);

    // Forced, the login is set aside where it can still be imported.
    let aside = set_default(dir, "other", true).unwrap().unwrap();
    assert_eq!(default_profile_name(dir).as_deref(), Some("other"));
    assert_eq!(identity(&aside).email.as_deref(), Some("first@example.com"));
    save(dir, "first", Some(&aside), false).unwrap();
}

#[test]
fn switching_the_default_between_saved_accounts_is_free() {
    let home = keyring(&["second"]);
    let dir = home.path();

    assert_eq!(set_default(dir, "second", false).unwrap(), None);
    assert_eq!(default_profile_name(dir).as_deref(), Some("second"));
    assert_eq!(
        what_an_older_cli_reads(dir).as_deref(),
        Some("second@example.com")
    );
    assert!(account_path(dir, "first").exists());
    assert!(set_default(dir, "missing", false).is_err());
}

#[test]
fn a_hand_kept_token_file_can_be_imported_without_touching_the_default() {
    let home = legacy_home();
    let dir = home.path();
    let kept = dir.join("token.operator");
    fs::write(
        &kept,
        token_file("operator@example.com", "org-9", "session-9"),
    )
    .unwrap();

    save(dir, "operator", Some(&kept), false).unwrap();
    assert_eq!(default_profile_name(dir), None);
    assert!(kept.exists());
    assert_eq!(
        identity(&account_path(dir, "operator")).email.as_deref(),
        Some("operator@example.com")
    );

    fs::write(&kept, "not a token").unwrap();
    assert!(save(dir, "junk", Some(&kept), false).is_err());
}

#[test]
fn removing_an_account_drops_its_bindings_and_never_strands_the_default() {
    let home = keyring(&["second"]);
    let dir = home.path();
    bind(dir, "app-1", Some("shop"), "first").unwrap();
    bind(dir, "app-2", None, "second").unwrap();

    // The default cannot go while another account could take its place.
    let err = remove(dir, "first").unwrap_err().to_string();
    assert!(err.contains("account use"), "{err}");

    let unbound = remove(dir, "second").unwrap();
    assert_eq!(unbound.len(), 1);
    assert_eq!(unbound[0].id, "app-2");
    assert!(!account_path(dir, "second").exists());

    // The last account takes the keyring's default with it.
    let unbound = remove(dir, "first").unwrap();
    assert_eq!(unbound[0].id, "app-1");
    assert_eq!(default_profile_name(dir), None);
    assert!(legacy_account_link(dir).symlink_metadata().is_err());
}

#[test]
fn logging_out_keeps_the_name_so_the_next_command_names_the_gap() {
    let home = keyring(&[]);
    let dir = home.path();
    bind(dir, "app-1", Some("shop"), "first").unwrap();

    let selection = resolve(dir, None, None, Some(("app-1", None))).unwrap();
    logout(dir, &selection).unwrap();

    assert!(!account_path(dir, "first").exists());
    assert_eq!(default_profile_name(dir).as_deref(), Some("first"));
    let after = resolve(dir, None, None, Some(("app-1", None))).unwrap();
    assert_eq!(after.name.as_deref(), Some("first"));
    assert!(!after.token_path.exists());
    // An older CLI finds no login either, not somebody else's.
    assert_eq!(what_an_older_cli_reads(dir), None);
}

#[test]
fn listing_labels_each_login_and_shows_an_unsaved_default() {
    let home = legacy_home();
    let dir = home.path();
    add(dir, "shop");
    bind(dir, "app-1", Some("shop-api"), "shop").unwrap();

    let accounts = list(dir).unwrap();
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].name.as_deref(), Some("shop"));
    assert_eq!(accounts[0].identity.kind, Some("api-key"));
    assert_eq!(
        accounts[0].identity.organization_id.as_deref(),
        Some("shop")
    );
    assert_eq!(
        accounts[0].applications[0].name.as_deref(),
        Some("shop-api")
    );
    assert!(!accounts[0].is_default);
    assert_eq!(accounts[1].name, None);
    assert!(accounts[1].is_default);
    assert_eq!(accounts[1].identity.kind, Some("session"));
}

#[test]
fn account_names_must_be_safe_as_file_names() {
    for good in [
        "pact",
        "rohin+pact",
        "shared.cluster",
        "a_b-c",
        "me@example.com",
    ] {
        assert!(validate_name(good).is_ok(), "{good}");
    }
    for bad in ["", "../etc", "a/b", ".hidden", "-flag", "with space"] {
        assert!(validate_name(bad).is_err(), "{bad}");
    }
    assert!(resolve(Path::new("/nonexistent"), Some("../x"), None, None).is_err());
}

#[cfg(unix)]
#[test]
fn stored_logins_are_readable_only_by_their_owner() {
    use std::os::unix::fs::PermissionsExt;
    let home = keyring(&[]);
    let mode = |path: PathBuf| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(account_path(home.path(), "first")), 0o600);
    assert_eq!(mode(profiles_dir(home.path())), 0o700);
}

// ── Older CLIs ────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn an_older_cli_is_handed_the_selected_account_and_the_default_comes_back() {
    let home = keyring(&["shop"]);
    let dir = home.path();

    let held = handoff(dir, &named(dir, "shop")).unwrap().unwrap();
    assert_eq!(
        what_an_older_cli_reads(dir).as_deref(),
        Some("shop@example.com")
    );
    // This CLI is not fooled by the pointer while it is borrowed.
    assert_eq!(
        resolve(dir, None, None, None).unwrap().name.as_deref(),
        Some("first")
    );

    drop(held);
    assert_eq!(
        what_an_older_cli_reads(dir).as_deref(),
        Some("first@example.com")
    );
}

#[cfg(unix)]
#[test]
fn older_clis_for_the_same_account_run_together_and_others_wait() {
    use std::{sync::mpsc, thread, time::Duration};
    let home = keyring(&["shop", "bank"]);
    let dir = home.path().to_path_buf();

    let first = handoff(&dir, &named(&dir, "shop")).unwrap().unwrap();
    let second = handoff(&dir, &named(&dir, "shop")).unwrap().unwrap();

    let (acquired, on_acquired) = mpsc::channel();
    let (release, on_release) = mpsc::channel::<()>();
    let waiter = {
        let dir = dir.clone();
        thread::spawn(move || {
            let held = handoff(&dir, &named(&dir, "bank")).unwrap().unwrap();
            acquired.send(what_an_older_cli_reads(&dir)).unwrap();
            on_release.recv().unwrap();
            drop(held);
        })
    };

    // While either `shop` command runs, `bank` waits and `shop` stays put.
    assert!(
        on_acquired
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
    drop(first);
    assert!(
        on_acquired
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
    assert_eq!(
        what_an_older_cli_reads(&dir).as_deref(),
        Some("shop@example.com")
    );

    drop(second);
    assert_eq!(
        on_acquired
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .as_deref(),
        Some("bank@example.com")
    );
    release.send(()).unwrap();
    waiter.join().unwrap();
    assert_eq!(
        what_an_older_cli_reads(&dir).as_deref(),
        Some("first@example.com")
    );
}

#[cfg(unix)]
#[test]
fn an_older_cli_is_not_pointed_away_from_an_unsaved_default_login() {
    let home = legacy_home();
    let dir = home.path();
    add(dir, "shop");

    let err = handoff(dir, &named(dir, "shop")).unwrap_err().to_string();
    assert!(err.contains("account save"), "{err}");
    assert_eq!(
        what_an_older_cli_reads(dir).as_deref(),
        Some("first@example.com")
    );
}

/// An older CLI used directly deletes the path when a login expires and
/// writes a plain file when it logs in. Neither may cost a login or
/// leave the default pointing nowhere.
#[cfg(unix)]
#[test]
fn what_an_older_cli_did_to_the_pointer_is_put_right() {
    let home = keyring(&[]);
    let dir = home.path();

    fs::remove_file(legacy_account_link(dir)).unwrap();
    assert_eq!(sync_pointer(dir, false).unwrap(), None);
    assert_eq!(
        what_an_older_cli_reads(dir).as_deref(),
        Some("first@example.com")
    );

    fs::remove_file(legacy_account_link(dir)).unwrap();
    fs::write(
        legacy_account_link(dir),
        token_file("stray@example.com", "org-7", "session-7"),
    )
    .unwrap();
    let aside = sync_pointer(dir, false).unwrap().unwrap();
    assert_eq!(identity(&aside).email.as_deref(), Some("stray@example.com"));
    assert_eq!(
        what_an_older_cli_reads(dir).as_deref(),
        Some("first@example.com")
    );
}
