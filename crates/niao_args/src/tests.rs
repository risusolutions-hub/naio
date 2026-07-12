use crate::{Arg, Command};

#[test]
fn env_fallback() {
    unsafe {
        std::env::set_var("NIAO_TEST_PORT", "4242");
    }
    let cmd = Command::new("app").arg(Arg::new("port").long("port").env("NIAO_TEST_PORT"));
    let m = cmd.try_get_matches_from(["app"]).unwrap();
    assert_eq!(m.get_one::<String>("port").as_deref(), Some("4242"));
    unsafe {
        std::env::remove_var("NIAO_TEST_PORT");
    }
}
