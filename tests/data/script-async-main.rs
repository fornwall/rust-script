//! This is merged into a default manifest in order to form the full package manifest:
//!
//! ```cargo
//! [dependencies]
//! boolinator = "=0.1.0"
//! tokio = { version = "1", features = ["full"] }
//! ```
use boolinator::Boolinator;

#[tokio::main]
async fn main() {
    println!("--output--");
    println!("{:?}", true.as_some(1));
}
