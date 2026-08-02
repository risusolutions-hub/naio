use niao_keyring as keyring;

#[test]
fn system_roundtrip_when_available() {
    keyring::use_system();
    let svc = "niao-keyring-test";
    let user = "integration";
    let _ = keyring::delete_password(svc, user);
    if keyring::set_password(svc, user, "test-secret").is_err() {
        eprintln!("skipping system integration: OS keyring unavailable");
        return;
    }
    assert_eq!(
        keyring::get_password(svc, user).unwrap(),
        Some("test-secret".into())
    );
    keyring::delete_password(svc, user).unwrap();
    assert_eq!(keyring::get_password(svc, user).unwrap(), None);
}
