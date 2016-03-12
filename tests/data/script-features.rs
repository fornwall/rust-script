/*!
```cargo
[features]
dont-panic = []
```
*/
#[cfg(feature="dont-panic")]
fn main() {
    println!("--output--");
    println!("Keep calm and borrow check.");
}

#[cfg(not(feature="dont-panic"))]
fn main() {
    panic!("Do I really exist from an external, non-subjective point of view?");
}
