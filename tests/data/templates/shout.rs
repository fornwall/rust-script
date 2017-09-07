#{prelude}

fn main() {
    match {#{script}} {
        script_result => {
            let text = script_result.to_string();
            let text = text.to_uppercase();
            println!("{}", text);
        }
    }
}
