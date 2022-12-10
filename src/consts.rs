/*!
This module just contains any big string literals I don't want cluttering up the rest of the code.
*/

pub const PROGRAM_NAME: &str = "rust-script";

/*
What follows are the templates used to wrap script input.
*/

/// Substitution for the script body.
pub const SCRIPT_BODY_SUB: &str = "script";

/// Substitution for the script prelude.
pub const SCRIPT_PRELUDE_SUB: &str = "prelude";

/// The template used for script file inputs.
pub const FILE_TEMPLATE: &str = r#"#{script}"#;

/// The template used for `--expr` input.
pub const EXPR_TEMPLATE: &str = r#"
#{prelude}
use std::any::{Any, TypeId};

fn main() {
    let exit_code = match try_main() {
        Ok(()) => None,
        Err(e) => {
            use std::io::{self, Write};
            let _ = writeln!(io::stderr(), "Error: {}", e);
            Some(1)
        },
    };
    if let Some(exit_code) = exit_code {
        std::process::exit(exit_code);
    }
}

fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    fn _rust_script_is_empty_tuple<T: ?Sized + Any>(_s: &T) -> bool {
        TypeId::of::<()>() == TypeId::of::<T>()
    }
    match {#{script}} {
        __rust_script_expr if !_rust_script_is_empty_tuple(&__rust_script_expr) => println!("{:?}", __rust_script_expr),
        _ => {}
    }
    Ok(())
}
"#;

/*
Regarding the loop templates: what I *want* is for the result of the closure to be printed to standard output *only* if it's not `()`.

* TODO: Merge the `LOOP_*` templates so there isn't duplicated code.  It's icky.
*/

/// The template used for `--loop` input, assuming no `--count` flag is also given.
pub const LOOP_TEMPLATE: &str = r#"
#![allow(unused_imports)]
#![allow(unused_braces)]
#{prelude}
use std::any::Any;
use std::io::prelude::*;

fn main() {
    let mut closure = enforce_closure(
{#{script}}
    );
    let mut line_buffer = String::new();
    let stdin = std::io::stdin();
    loop {
        line_buffer.clear();
        let read_res = stdin.read_line(&mut line_buffer).unwrap_or(0);
        if read_res == 0 { break }
        let output = closure(&line_buffer);

        let display = {
            let output_any: &dyn Any = &output;
            !output_any.is::<()>()
        };

        if display {
            println!("{:?}", output);
        }
    }
}

fn enforce_closure<F, T>(closure: F) -> F
where F: FnMut(&str) -> T, T: 'static {
    closure
}
"#;

/// The template used for `--count --loop` input.
pub const LOOP_COUNT_TEMPLATE: &str = r#"
#![allow(unused_imports)]
#![allow(unused_braces)]
use std::any::Any;
use std::io::prelude::*;

fn main() {
    let mut closure = enforce_closure(
{#{script}}
    );
    let mut line_buffer = String::new();
    let stdin = std::io::stdin();
    let mut count = 0;
    loop {
        line_buffer.clear();
        let read_res = stdin.read_line(&mut line_buffer).unwrap_or(0);
        if read_res == 0 { break }
        count += 1;
        let output = closure(&line_buffer, count);

        let display = {
            let output_any: &dyn Any = &output;
            !output_any.is::<()>()
        };

        if display {
            println!("{:?}", output);
        }
    }
}

fn enforce_closure<F, T>(closure: F) -> F
where F: FnMut(&str, usize) -> T, T: 'static {
    closure
}
"#;

/// Substitution for the identifier-safe package name of the script.
pub const MANI_NAME_SUB: &str = "name";

/// Substitution for the identifier-safe bin name of the script.
pub const MANI_BIN_NAME_SUB: &str = "bin_name";

/// Substitution for the filesystem-safe name of the script.
pub const MANI_FILE_SUB: &str = "file";

/**
The default manifest used for packages.
*/
#[rustversion::before(1.59)]
pub const DEFAULT_MANIFEST: &str = r##"
[package]
name = "#{name}"
version = "0.1.0"
authors = ["Anonymous"]
edition = "2021"

[[bin]]
name = "#{bin_name}"
path = "#{file}.rs"
"##;
#[rustversion::since(1.59)]
pub const DEFAULT_MANIFEST: &str = r##"
[package]
name = "#{name}"
version = "0.1.0"
authors = ["Anonymous"]
edition = "2021"

[[bin]]
name = "#{bin_name}"
path = "#{file}.rs"

[profile.release]
strip = true
"##;

/**
When generating a package's unique ID, how many hex nibbles of the digest should be used *at most*?

The largest meaningful value is `40`.
*/
pub const ID_DIGEST_LEN_MAX: usize = 24;

/**
How old can stuff in the cache be before we automatically clear it out?

Measured in milliseconds.
*/
// It's been *one week* since you looked at me,
// cocked your head to the side and said "I'm angry."
pub const MAX_CACHE_AGE_MS: u128 = 7 * 24 * 60 * 60 * 1000;
