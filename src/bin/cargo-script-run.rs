/*!
`cargo-script-run` is a trampoline program designed to be used as the target of hashbangs and/or file associations to allow Rust source files to be executed as scripts.

The invocation is simple: it expects *at least* one argument, which is passed to `cargo-script` as the name of a script file to execute.  All other arguments are passed *after* a `--` argument to ensure they are interpreted as arguments to the *script* as opposed to this program, `cargo` or `cargo-script`.

# Why a separate program?

Because `cargo-script` isn't suitable for use as a hashbang target.  There are *two* problems:

1. Because you *should* be using `/usr/bin/env` to locate the interpreter, you can't pass arguments.
2. Without arguments, you can't distinguish between `cargo-script script` as the user trying to invoke a script file literally named `script` and a user executing `cargo script` with no further arguments.

In addition, making it a program instead of a shell script means it doesn't have to worry about things like what shell the user has, or what happens on Windows.
*/
use std::process::Command;

fn main() {
    let mut args = std::env::args();
    let exe = args.next().unwrap_or_else(|| "cargo-script-run".into());
    let path = match args.next() {
        Some(v) => v,
        None => {
            use std::io::Write;
            let stderr = &mut std::io::stderr();
            let _ = writeln!(stderr, "Usage: {} PATH", exe);
            std::process::exit(1);
        }
    };

    let mut cmd = Command::new("cargo");

    cmd.arg("script")
        .arg(path)
        .arg("--");

    for arg in args {
        cmd.arg(arg);
    }

    let exit_status = match match cmd.status() {
        Ok(st) => st.code(),
        Err(_) => None,
    } {
        Some(c) => c,
        None => !0
    };
    std::process::exit(exit_status);
}
