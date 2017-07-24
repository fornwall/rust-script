#[test]
fn test_script_explicit() {
    let out = cargo_script!("-dboolinator", "tests/data/script-explicit.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    ).unwrap()
}

#[test]
fn test_script_features() {
    let out = cargo_script!("--features", "dont-panic", "tests/data/script-features.rs").unwrap();
    scan!(out.stdout_output();
        ("Keep calm and borrow check.") => ()
    ).unwrap();

    let out = cargo_script!("tests/data/script-features.rs").unwrap();
    assert!(!out.success());
}

#[test]
fn test_script_full_block() {
    let out = cargo_script!("tests/data/script-full-block.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    ).unwrap()
}

#[test]
fn test_script_full_line() {
    let out = cargo_script!("tests/data/script-full-line.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    ).unwrap()
}

#[test]
fn test_script_invalid_doc_comment() {
    let out = cargo_script!("tests/data/script-invalid-doc-comment.rs").unwrap();
    scan!(out.stdout_output();
        ("Hello, World!") => ()
    ).unwrap()
}

#[test]
fn test_script_no_deps() {
    let out = cargo_script!("tests/data/script-no-deps.rs").unwrap();
    scan!(out.stdout_output();
        ("Hello, World!") => ()
    ).unwrap()
}

#[test]
fn test_script_short() {
    let out = cargo_script!("tests/data/script-short.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    ).unwrap()
}
