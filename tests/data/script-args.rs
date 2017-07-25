fn main() {
    println!("--output--");
    for (i, arg) in std::env::args().enumerate() {
        println!("{:>4}: {:?}", format!("[{}]", i), arg);
    }
}
