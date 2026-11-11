#[test]
fn crate_version_is_set() {
    assert_eq!(rp2040rs::VERSION, env!("CARGO_PKG_VERSION"));
}
