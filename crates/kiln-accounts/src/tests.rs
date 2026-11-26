use super::*;

fn account(id: &str, email: &str) -> Account {
    Account {
        id: id.into(),
        email: email.into(),
        plan: "Claude Pro".into(),
        agent: "claude".into(),
    }
}

#[test]
fn an_added_account_has_a_credential_store_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let mut accounts = Accounts::load(dir.path().to_owned()).unwrap();
    let (id, env) = accounts.reserve("claude").unwrap();
    assert!(id.starts_with("account-"), "{id}");
    let store = dir.path().join("accounts").join(&id);
    assert_eq!(
        env,
        [(
            CREDENTIALS_VAR.to_owned(),
            store.to_string_lossy().into_owned()
        )]
    );
    assert!(store.is_dir());
    accounts.add(account(&id, "a@example.com")).unwrap();
    assert_ne!(accounts.reserve("claude").unwrap().0, id);
}

#[test]
fn added_accounts_are_kept_across_loads() {
    let dir = tempfile::tempdir().unwrap();
    let mut accounts = Accounts::load(dir.path().to_owned()).unwrap();
    let (id, _) = accounts.reserve("claude").unwrap();
    accounts.add(account(&id, "a@example.com")).unwrap();
    let again = Accounts::load(dir.path().to_owned()).unwrap();
    assert_eq!(again.added(), [account(&id, "a@example.com")]);
    assert_eq!(again.find(&id), Some(&account(&id, "a@example.com")));
    assert_eq!(
        again
            .find_email("claude", "A@example.com")
            .map(|a| a.id.as_str()),
        Some(id.as_str())
    );
}

#[test]
fn an_unfinished_sign_in_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = Accounts::load(dir.path().to_owned()).unwrap();
    let (id, _) = accounts.reserve("claude").unwrap();
    accounts.release(&id);
    assert!(!dir.path().join("accounts").join(&id).exists());
    assert_eq!(accounts.added(), Vec::new());
}

#[test]
fn a_damaged_list_is_an_error_not_an_empty_list() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(LIST_FILE), "{").unwrap();
    assert!(Accounts::load(dir.path().to_owned()).is_err());
}

#[test]
fn reported_usage_is_kept_across_loads_for_added_accounts_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut accounts = Accounts::load(dir.path().to_owned()).unwrap();
    let (id, _) = accounts.reserve("claude").unwrap();
    accounts.add(account(&id, "a@example.com")).unwrap();
    let known = KnownUsage {
        usage: kiln_agent::Usage {
            session: Some(kiln_agent::UsageWindow {
                percent: 42,
                resets_at: Some(UNIX_EPOCH + std::time::Duration::from_hours(1)),
            }),
            ..kiln_agent::Usage::default()
        },
        reported_at: UNIX_EPOCH,
    };
    accounts.set_usage(&id, known).unwrap();
    accounts.set_usage("account-unknown", known).unwrap();
    let again = Accounts::load(dir.path().to_owned()).unwrap();
    assert_eq!(again.usage(&id), Some(known));
    assert_eq!(again.usage("account-unknown"), None);
}

#[test]
fn damaged_usage_is_forgotten_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(USAGE_FILE), "{").unwrap();
    let accounts = Accounts::load(dir.path().to_owned()).unwrap();
    assert_eq!(accounts.usage("account-1"), None);
}

#[test]
fn accounts_kept_before_codex_are_claudes() {
    let dir = tempfile::tempdir().unwrap();
    let kept = r#"[{"id": "account-1", "email": "a@example.com", "plan": "Claude Pro"}]"#;
    std::fs::write(dir.path().join(LIST_FILE), kept).unwrap();
    let accounts = Accounts::load(dir.path().to_owned()).unwrap();
    assert_eq!(accounts.added_for("claude").count(), 1);
    assert_eq!(accounts.added_for("codex").count(), 0);
    assert_eq!(accounts.find_email("codex", "a@example.com"), None);
}

#[test]
fn a_codex_account_has_a_codex_home_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = Accounts::load(dir.path().to_owned()).unwrap();
    let (id, env) = accounts.reserve("codex").unwrap();
    let home = dir.path().join("accounts").join(&id);
    assert_eq!(
        env[0],
        ("CODEX_HOME".to_owned(), home.to_string_lossy().into_owned())
    );
}
