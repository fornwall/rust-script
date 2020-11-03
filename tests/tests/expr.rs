#[test]
fn test_expr_0() {
    let out = rust_script!("-e", with_output_marker!("0")).unwrap();
    scan!(out.stdout_output();
        ("0") => ()
    )
    .unwrap()
}

#[test]
fn test_expr_comma() {
    let out = rust_script!("-e", with_output_marker!("[1, 2, 3]")).unwrap();
    scan!(out.stdout_output();
        ("[1, 2, 3]") => ()
    )
    .unwrap()
}

#[test]
fn test_expr_dnc() {
    let out = rust_script!("-e", "swing begin").unwrap();
    assert!(!out.success());
}

#[test]
fn test_expr_temporary() {
    let out = rust_script!("-e", "[1].iter().max()").unwrap();
    assert!(out.success());
}

#[test]
fn test_expr_dep() {
    let out = rust_script!(
        "-d",
        "boolinator=0.1.0",
        "-e",
        with_output_marker!(
            prelude "use boolinator::Boolinator;";
            "true.as_some(1)"
        )
    )
    .unwrap();
    scan!(out.stdout_output();
    ("Some(1)") => ()
    )
    .unwrap();
}

#[test]
fn test_expr_panic() {
    let out = rust_script!("-e", with_output_marker!("panic!()")).unwrap();
    assert!(!out.success());
}

#[test]
fn test_expr_qmark() {
    let code = with_output_marker!("\"42\".parse::<i32>()?.wrapping_add(1)");
    let out = rust_script!("-e", code).unwrap();
    scan!(out.stdout_output();
        ("43") => ()
    )
    .unwrap();
}

#[test]
fn test_expr_template() {
    let template_dir = "tests/data/templates";
    let out = rust_script!(
        #[env(RUST_SCRIPT_DEBUG_TEMPLATE_PATH=template_dir)]
        "-t",
        "shout",
        "-e",
        with_output_marker!(r#""no way? no way!""#)
    )
    .unwrap();
    scan!(out.stdout_output();
        ("NO WAY? NO WAY!") => ()
    )
    .unwrap();
}

#[test]
fn test_expr_template_with_deps() {
    let template_dir = "tests/data/templates";
    let out = rust_script!(
        #[env(RUST_SCRIPT_DEBUG_TEMPLATE_PATH=template_dir)]
        "-t",
        "boolinate",
        "-e",
        with_output_marker!(r#"true"#)
    )
    .unwrap();
    scan!(out.stdout_output();
        ("Some(())") => ()
    )
    .unwrap();
}

#[test]
fn test_expr_template_override_expr() {
    let template_dir = "tests/data/templates/override";
    let out = rust_script!(
        #[env(RUST_SCRIPT_DEBUG_TEMPLATE_PATH=template_dir)]
        "-e",
        with_output_marker!(r#"true"#)
    )
    .unwrap();
    scan!(out.stdout_output();
        ("Some(())") => ()
    )
    .unwrap();
}
