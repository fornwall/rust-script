use std::env;

fn main() {
    println!("--output--");
    let path = env::var("CARGO_SCRIPT_SCRIPT_PATH").expect("CSSP wasn't set");
    assert!(path.ends_with("script-cs-env.rs"));
    assert_eq!(env::var("CARGO_SCRIPT_SAFE_NAME"), Ok("script-cs-env".into()));
    assert_eq!(env::var("CARGO_SCRIPT_PKG_NAME"), Ok("script-cs-env".into()));
    let base_path = env::var("CARGO_SCRIPT_BASE_PATH").expect("CSBP wasn't set");
    assert!(base_path.ends_with("data"));
    println!("Ok");
}
