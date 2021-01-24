//! ```cargo
//! [dependencies]
//! boolinator = "=0.1.0"
//! ```
use boolinator::Boolinator;

pub fn main() {
    println!("--output--");
    println!("{:?}", true.as_some(1));
}
