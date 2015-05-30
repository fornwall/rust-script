/*!
This module just contains any big string literals I don't want cluttering up the rest of the code.

The `USAGE` string is exempt, since that explains what `Args` means.
*/

/*
What follows are the templates used to wrap script input.  The input provided by the user is inserted in place of `%%`.  *Proper* templates of some kind would be nice, but damnit, I'm lazy.
*/

/// The template used for script file inputs.
pub const FILE_TEMPLATE: &'static str = r#"%%"#;

/// The template used for `--expr` input.
pub const EXPR_TEMPLATE: &'static str = r#"
fn main() {
    println!("{}", (%%));
}
"#;

/*
Regarding the loop templates: what I *want* is for the result of the closure to be printed to standard output *only* if it's not `()`.

* TODO: Just use TypeId, dumbass.
* TODO: Merge the `LOOP_*` templates so there isn't duplicated code.  It's icky.
*/

/// The template used for `--loop` input, assuming no `--count` flag is also given.
pub const LOOP_TEMPLATE: &'static str = r#"
use std::io::prelude::*;

fn main() {
    let mut out_buffer: Vec<u8> = vec![];
    let mut line_buffer = String::new();
    let mut stdin = std::io::stdin();
    loop {
        line_buffer.clear();
        let read_res = stdin.read_line(&mut line_buffer).unwrap_or(0);
        if read_res == 0 { break }
        let output = invoke_closure(&line_buffer, %%);

        out_buffer.clear();
        write!(&mut out_buffer, "{:?}", output).unwrap();
        let out_str = String::from_utf8_lossy(&out_buffer);
        if &*out_str != "()" {
            println!("{}", out_str);
        }
    }
}

fn invoke_closure<F, T>(line: &str, mut closure: F) -> T
where F: FnMut(&str) -> T {
    closure(line)
}
"#;

/// The template used for `--count --loop` input.
pub const LOOP_COUNT_TEMPLATE: &'static str = r#"
use std::io::prelude::*;

fn main() {
    let mut out_buffer: Vec<u8> = vec![];
    let mut line_buffer = String::new();
    let mut stdin = std::io::stdin();
    let mut count = 0;
    loop {
        line_buffer.clear();
        let read_res = stdin.read_line(&mut line_buffer).unwrap_or(0);
        if read_res == 0 { break }
        count += 1;
        let output = invoke_closure(&line_buffer, count, %%);

        out_buffer.clear();
        write!(&mut out_buffer, "{:?}", output).unwrap();
        let out_str = String::from_utf8_lossy(&out_buffer);
        if &*out_str != "()" {
            println!("{}", out_str);
        }
    }
}

fn invoke_closure<F, T>(line: &str, count: usize, mut closure: F) -> T
where F: FnMut(&str, usize) -> T {
    closure(line, count)
}
"#;

/**
The default manifest used for packages.  `%n` is replaced with the "safe name" of the input, which *should* be safe to use as a file name.
*/
pub const DEFAULT_MANIFEST: &'static str = r#"
[package]
name = "%n"
version = "0.1.0"
authors = ["Anonymous"]

[[bin]]
name = "%n"
path = "%n.rs"
"#;

/**
The name of the package metadata file.
*/
pub const METADATA_FILE: &'static str = "metadata.json";

/**
Extensions to check when trying to find script input by name.
*/
pub const SEARCH_EXTS: &'static [&'static str] = &["crs", "rs"];

/*
These relate to Input::compute_id.
*/

/**
When generating a package's unique ID, how many hex nibbles of the compressed path should be used *at most*?
*/
pub const DEFLATE_PATH_LEN_MAX: usize = 20;

/**
When generating a package's unique ID, how many hex nibbles of the script digest should be used *at most*?
*/
pub const CONTENT_DIGEST_LEN_MAX: usize = 20;
