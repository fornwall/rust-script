#[test]
fn test_script_explicit() {
    let out = rust_script!("-d", "boolinator", "tests/data/script-explicit.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    )
    .unwrap()
}

#[test]
fn test_script_full_block() {
    let out = rust_script!("tests/data/script-full-block.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    )
    .unwrap()
}

#[test]
fn test_script_full_line() {
    let out = rust_script!("tests/data/script-full-line.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    )
    .unwrap()
}

#[test]
fn test_script_full_line_without_main() {
    let out = rust_script!("tests/data/script-full-line-without-main.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    )
    .unwrap()
}

#[test]
fn test_script_invalid_doc_comment() {
    let out = rust_script!("tests/data/script-invalid-doc-comment.rs").unwrap();
    scan!(out.stdout_output();
        ("Hello, World!") => ()
    )
    .unwrap()
}

#[test]
fn test_script_no_deps() {
    let out = rust_script!("tests/data/script-no-deps.rs").unwrap();
    scan!(out.stdout_output();
        ("Hello, World!") => ()
    )
    .unwrap()
}

#[test]
fn test_script_short() {
    let out = rust_script!("tests/data/script-short.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    )
    .unwrap()
}

#[test]
fn test_script_short_without_main() {
    let out = rust_script!("tests/data/script-short-without-main.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    )
    .unwrap()
}

#[test]
fn test_script_test() {
    let out = rust_script!("--test", "tests/data/script-test.rs").unwrap();
    assert!(out.success());
}

#[test]
fn test_script_hyphens() {
    use scan_rules::scanner::QuotedString;
    let out = rust_script!("--", "tests/data/script-args.rs", "-NotAnArg").unwrap();
    scan!(out.stdout_output();
        ("[0]:", let _: QuotedString, "[1]:", let arg: QuotedString) => {
            assert_eq!(arg, "-NotAnArg");
        }
    )
    .unwrap()
}

#[test]
fn test_script_hyphens_without_separator() {
    use scan_rules::scanner::QuotedString;
    let out = rust_script!("tests/data/script-args.rs", "-NotAnArg").unwrap();
    scan!(out.stdout_output();
        ("[0]:", let _: QuotedString, "[1]:", let arg: QuotedString) => {
            assert_eq!(arg, "-NotAnArg");
        }
    )
    .unwrap()
}

#[test]
fn test_script_has_weird_chars() {
    let out = rust_script!("tests/data/script-has.weird§chars!.rs").unwrap();
    assert!(out.success());
}

#[test]
fn test_script_cs_env() {
    let out = rust_script!("tests/data/script-cs-env.rs").unwrap();
    scan!(out.stdout_output();
        ("Ok") => ()
    )
    .unwrap()
}

#[test]
fn test_script_including_relative() {
    let out = rust_script!("tests/data/script-including-relative.rs").unwrap();
    scan!(out.stdout_output();
        ("hello, including script") => ()
    )
    .unwrap()
}

#[test]
fn script_with_same_name_as_dependency() {
    let out = rust_script!("tests/data/time.rs").unwrap();
    scan!(out.stdout_output();
        ("Hello") => ()
    )
    .unwrap()
}

#[test]
fn script_without_main_question_mark() {
    let out = rust_script!("tests/data/question-mark").unwrap();
    assert!(out
        .stderr
        .starts_with("Error: Os { code: 2, kind: NotFound, message:"));
}

#[test]
fn test_script_async_main() {
    let out = rust_script!("tests/data/script-async-main.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    )
    .unwrap()
}

#[test]
fn test_pub_fn_main() {
    let out = rust_script!("tests/data/pub-fn-main.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    )
    .unwrap()
}

#[test]
fn test_cargo_target_dir_env() {
    let out = rust_script!("tests/data/cargo-target-dir-env.rs").unwrap();
    scan!(out.stdout_output();
        ("true") => ()
    )
    .unwrap()
}

#[test]
fn test_outer_line_doc() {
    let out = rust_script!("tests/data/outer-line-doc.rs").unwrap();
    scan!(out.stdout_output();
        ("Some(1)") => ()
    )
    .unwrap()
}

#[test]
fn test_whitespace_before_main() {
    let out = rust_script!("tests/data/whitespace-before-main.rs").unwrap();
    scan!(out.stdout_output();
        ("hello, world") => ()
    )
    .unwrap()
}

#[test]
#[ignore]
fn test_stable_toolchain() {
    let out = rust_script!(
        "--toolchain",
        "stable",
        "tests/data/script-unstable-feature.rs"
    )
    .unwrap();
    assert!(out.stderr.contains("`#![feature]` may not be used"));
    assert!(!out.success());
}

#[test]
#[ignore]
fn test_nightly_toolchain() {
    let out = rust_script!(
        "--toolchain",
        "nightly",
        "tests/data/script-unstable-feature.rs"
    )
    .unwrap();
    scan!(out.stdout_output();
        ("`#![feature]` *may* be used!") => ()
    )
    .unwrap();
    assert!(out.success());
}

#[test]
fn test_same_flags() {
    let out = rust_script!("tests/data/same-flags.rs", "--help").unwrap();
    scan!(out.stdout_output();
        ("Argument: --help") => ()
    )
    .unwrap()
}
