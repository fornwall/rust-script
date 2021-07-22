use std::env;

pub fn main() {
    // Test that CARGO_TARGET_DIR is not set by rust-script to avoid
    // interfering with cargo calls done by the script.
    // See https://github.com/fornwall/rust-script/issues/27
    let env_variable = env::var("CARGO_TARGET_DIR");
    println!("--output--");
    println!(
        "{:?}",
        matches!(env_variable, Err(env::VarError::NotPresent))
    );
}
