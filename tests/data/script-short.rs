// cargo-deps: boolinator="=0.1.0"
// You can also leave off the version number, in which case, it's assumed
// to be "*".  Also, the `cargo-deps` comment *must* be a single-line
// comment, and it *must* be the first thing in the file, after the
// hashbang.
extern crate boolinator;
use boolinator::Boolinator;
fn main() {
    println!("--output--");
    println!("{:?}", true.as_some(1));
}
