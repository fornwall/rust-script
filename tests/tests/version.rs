#[test]
fn test_version() {
    let out = cargo_script!("--version").unwrap();
    assert!(out.success());
    scan!(&out.stdout;
        ("rust-script", &::std::env::var("CARGO_PKG_VERSION").unwrap(), .._) => ()
    )
    .unwrap();
}
