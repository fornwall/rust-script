let msg = option_env!("_RUST_SCRIPT_TEST_MESSAGE").unwrap_or("undefined");

println!("--output--");
println!("msg = {}", msg);