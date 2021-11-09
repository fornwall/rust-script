println!("--output--");
if let Some(arg) = std::env::args().skip(1).next() {
    println!("Argument: {}", arg);
}
