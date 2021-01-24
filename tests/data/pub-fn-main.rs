//! [dependencies]
//! boolinator = "=0.1.0"
//! tokio = { version = "1", features = ["full"] }
use boolinator::Boolinator;

#[tokio::main]
pub fn main() {
    println!("--output--");
    println!("{:?}", true.as_some(1));
}
