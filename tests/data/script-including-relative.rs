use std::env;

fn main() {
    println!("--output--");
    let s = include_str!(concat!(env!("RUST_SCRIPT_BASE_PATH"), "/file-to-be-included.txt"));
    println!("{}", s);
}
