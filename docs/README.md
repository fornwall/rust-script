<style>
  ul li:not(:last-child) { margin-bottom: 0.4em; }
</style>

- [Overview](#overview)
- [News](#news)
- [Installation](#installation)
  - [Distro Packages](#distro-packages)
    - [Arch Linux](#arch-linux)
- [Scripts](#scripts)
- [Executable Scripts](#executable-scripts)
- [Expressions](#expressions)
- [Filters](#filters)
- [Environment Variables](#environment-variables)
- [Troubleshooting](#troubleshooting)

## Overview

With `rust-script` Rust files and expressions can be executed just like a shell or Python script. Features include:

- Caching compiled artifacts for speed.
- Reading Cargo manifests embedded in Rust scripts.
- Supporting executable Rust scripts via Unix shebangs and Windows file associations.
- Using expressions as stream filters (*i.e.* for use in command pipelines).
- Running unit tests and benchmarks from scripts.

You can get an overview of the available options using the `--help` flag.

## News
**2023-04-14:** [Version 0.26.0](https://github.com/fornwall/rust-script/releases/tag/0.26.0) has been released, detecting `extern "C"` main functions.

**2023-04-11:** [Version 0.25.0](https://github.com/fornwall/rust-script/releases/tag/0.25.0) has been released, fixing whitespace between `main` and `()` not working, and avoids having shebangs cause a line number mismatch.

**2023-04-05:** [Version 0.24.0](https://github.com/fornwall/rust-script/releases/tag/0.24.0) has been released, containing a fix for Windows executable caching not working.

**2023-03-25:** [Version 0.23.0](https://github.com/fornwall/rust-script/releases/tag/0.23.0) has been released, bringing improved performance on subsequent runs, flexibility using `-p`/`--package` by printing the path to the generated package and avoids breakage due to rust toolchain files.

## Installation

Install or update `rust-script` using Cargo:

```sh
cargo install rust-script
```

Rust 1.64 or later is required.

### Distro Packages

#### Arch Linux

`rust-script` can be installed from the [community repository](https://archlinux.org/packages/community/x86_64/rust-script/):

```sh
pacman -S rust-script
```

## Scripts

The primary use for `rust-script` is for running Rust source files as scripts. For example:

```sh
$ echo 'println!("Hello, World!");' > hello.rs
$ rust-script hello.rs
Hello, World!
```

Under the hood, a Cargo project will be generated and built (with the Cargo output hidden unless compilation fails or the `-c`/`--cargo-output` option is used). The first invocation of the script will be slower as the script is compiled - subsequent invocations of unmodified scripts will be fast as the built executable is cached.

As seen from the above example, using a `fn main() {}` function is not required. If not present, the script file will be wrapped in a `fn main() { ... }` block.

`rust-script` will look for embedded dependency and manifest information in the script as shown by the below two equivalent `now.rs` variants:

```rust
#!/usr/bin/env rust-script
//! This is a regular crate doc comment, but it also contains a partial
//! Cargo manifest.  Note the use of a *fenced* code block, and the
//! `cargo` "language".
//!
//! ```cargo
//! [dependencies]
//! time = "0.1.25"
//! ```
fn main() {
    println!("{}", time::now().rfc822z());
}
```

```rust
// cargo-deps: time="0.1.25"
// You can also leave off the version number, in which case, it's assumed
// to be "*".  Also, the `cargo-deps` comment *must* be a single-line
// comment, and it *must* be the first thing in the file, after the
// shebang.
// Multiple dependencies should be separated by commas:
// cargo-deps: time="0.1.25", libc="0.2.5"
fn main() {
    println!("{}", time::now().rfc822z());
}
```

The output from running one of the above scripts may look something like:

```sh
$ rust-script now
Wed, 28 Oct 2020 00:38:45 +0100
```

Useful command-line arguments:

- `--bench`: Compile and run benchmarks. Requires a nightly toolchain.
- `--debug`: Build a debug executable, not an optimised one.
- `--force`: Force the script to be rebuilt.  Useful if you want to force a recompile with a different toolchain.
- `--package`: Generate the Cargo package and print the path to it - but don't compile or run it. Effectively "unpacks" the script into a Cargo package.
- `--test`: Compile and run tests.

## Executable Scripts

On Unix systems, you can use `#!/usr/bin/env rust-script` as a shebang line in a Rust script.  This will allow you to execute a script files (which don't need to have the `.rs` file extension) directly.

If you are using Windows, you can associate the `.ers` extension (executable Rust - a renamed `.rs` file) with `rust-script`.  This allows you to execute Rust scripts simply by naming them like any other executable or script.

This can be done using the `rust-script --install-file-association` command. Uninstall the file association with `rust-script --uninstall-file-association`.

If you want to make a script usable across platforms, use *both* a shebang line *and* give the file a `.ers` file extension.

## Expressions

Using the `-e`/`--expr` option a Rust expression can be evaluated directly, with dependencies (if any) added using `-d`/`--dep`:

```sh
$ rust-script -e '1+2'
3
$ rust-script --dep time --expr "time::OffsetDateTime::now_utc().format(time::Format::Rfc3339).to_string()"`
"2020-10-28T11:42:10+00:00"
$ # Use a specific version of the time crate (instead of default latest):
$ rust-script --dep time=0.1.38 -e "time::now().rfc822z().to_string()"
"2020-10-28T11:42:10+00:00"
```

The code given is embedded into a block expression, evaluated, and printed out using the `Debug` formatter (*i.e.* `{:?}`).

## Filters

You can use `rust-script` to write a quick filter, by specifying a closure to be called for each line read from stdin, like so:

```sh
$ cat now.ers | rust-script --loop \
    "let mut n=0; move |l| {n+=1; println!(\"{:>6}: {}\",n,l.trim_end())}"
     1: // cargo-deps: time="0.1.25"
     3: fn main() {
     4:     println!("{}", time::now().rfc822z());
     5: }
```

You can achieve a similar effect to the above by using the `--count` flag, which causes the line number to be passed as a second argument to your closure:

```sh
$ cat now.ers | rust-script --count --loop \
    "|l,n| println!(\"{:>6}: {}\", n, l.trim_end())"
     1: // cargo-deps: time="0.1.25"
     2: fn main() {
     3:     println!("{}", time::now().rfc822z());
     4: }
```

## Environment Variables

The following environment variables are provided to scripts by `rust-script`:

- `RUST_SCRIPT_BASE_PATH`: the base path used by `rust-script` to resolve relative dependency paths.  Note that this is *not* necessarily the same as either the working directory, or the directory in which the script is being compiled.

- `RUST_SCRIPT_PKG_NAME`: the generated package name of the script.

- `RUST_SCRIPT_SAFE_NAME`: the file name of the script (sans file extension) being run.  For scripts, this is derived from the script's filename.  May also be `"expr"` or `"loop"` for those invocations.

- `RUST_SCRIPT_PATH`: absolute path to the script being run, assuming one exists.  Set to the empty string for expressions.

## Troubleshooting

Please report all issues on [the GitHub issue tracker](https://github.com/fornwall/rust-script/issues).

If relevant, run with the `RUST_LOG=rust_script=trace` environment variable set to see verbose log output and attach that output to an issue.
