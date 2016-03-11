#[test]
fn test_expr_0() {
    let out = cargo_script!("-e", with_output_marker!("0")).unwrap();
    scan!(out.stdout_output();
        ("0") => ()
    ).unwrap()
}

#[test]
fn test_expr_dnc() {
    let out = cargo_script!("-e", "swing begin").unwrap();
    assert!(!out.success());
}

#[test]
fn test_expr_panic() {
    let out = cargo_script!("-e", with_output_marker!("panic!()")).unwrap();
    assert!(!out.success());
}
