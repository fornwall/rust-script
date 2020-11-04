use std::env;

mod script_module {
    include!(concat!(env!("RUST_SCRIPT_BASE_PATH"), "/script-module.rs"));
}

fn main() {
    println!("--output--");
    let s = include_str!(concat!(env!("RUST_SCRIPT_BASE_PATH"), "/file-to-be-included.txt"));
    assert_eq!(script_module::A_VALUE, 1);
    println!("{}", s);
}
