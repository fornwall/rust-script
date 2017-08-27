fn main() {
    println!("Sleeping for 2 seconds...");
    std::thread::sleep(std::time::Duration::from_millis(2000));
    println!("Done.");
}
