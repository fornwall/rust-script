# `cargo-script`

`cargo-script` is a Cargo subcommand designed to let people quickly and easily run Rust "scripts" which can make use of Cargo's package ecosystem.

Or, to put it in other words, it lets you write useful, but small, Rust programs without having to create a new directory and faff about with `Cargo.toml`.

As such, `cargo-script` does two major things:

1. Given a script, it extracts the embedded Cargo manifest and merges it with some sensible defaults.  This manifest, along with the source code, is written to a fresh Cargo package on-disk.

2. It caches the generated and compiled packages, regenerating them only if the script or its metadata have changed.

## Compiling and Installation

Clone the repository using `git clone --recursive` in order to pull in the required submodule.  If you've *already* cloned, but didn't use `--recursive`, you can run `git submodule update --init --recursive`.

`cargo-script` requires a nightly build of `rustc` due to the use of several unstable features.  Aside from that, it should build cleanly with `cargo build --release`.

Once built, you should place the resulting executable somewhere on your `PATH`.  At that point, you should be able to invoke it by using `cargo script`.

Note that you *can* run the executable directly, but the first argument will *need* to be `script`.

If you want to run `cargo script` from a hashbang, you should also install the `run-cargo-script` program.  We *strongly* recommend installing this program to the `PATH` and using `#!/usr/bin/env run-cargo-script` as the hashbang line.

## Usage

The simplest way to use `cargo-script` is to simply pass it the name of the Rust script you want to execute:

```shell
$ echo 'fn main() { println!("Hello, World!"); }' > hello.rs
$ cargo script hello.rs
   Compiling hello v0.1.0 (file:///C:/Users/drk/AppData/Local/Cargo/script-cache/file-hello-25c8c198030c5d089740-3ace88497b98af47db6e)
Hello, World!
$ cargo script hello # you can omit the file extension
Hello, World!
```

Note that `cargo-script` does not *currently* do anything to suppress the regular output of Cargo.  This is *definitely* on purpose and *not* simply out of abject laziness.

You may also embed a partial Cargo manifest at the start of your script, as shown below.  `cargo-script` specifically supports the `.crs` extension to distinguish such "Cargoified" files from regular Rust source, but it will process regular `.rs` files in *exactly* the same manner.

Note that all of the following are equivalent:

`now.rs` (code block manifest *and* UNIX hashbang):

```rust
#!/usr/bin/env run-cargo-script
//! This is a regular crate doc comment, but it also contains a partial
//! Cargo manifest.  Note the use of a *fenced* code block, and the
//! `cargo` "language".
//!
//! ```cargo
//! [dependencies]
//! time = "0.1.25"
//! ```
extern crate time;
fn main() {
    println!("{}", time::now().rfc822z());
}
```

`now.rs` (dependency-only, short-hand manifest):

```rust
// cargo-deps: time="0.1.25"
// You can also leave off the version number, in which case, it's assumed
// to be "*".  Also, the `cargo-deps` comment *must* be a single-line
// comment, and it *must* be the first thing in the file, after the
// hashbang.
extern crate time;
fn main() {
    println!("{}", time::now().rfc822z());
}
```

`now.crs` (prefix manifest; *these might be removed in the future*):

```rust
[dependencies]
time = "0.1.25"
---
extern crate time;
fn main() {
    println!("{}", time::now().rfc822z());
}
```

```shell
$ cargo script now
    Updating registry `https://github.com/rust-lang/crates.io-index`
   Compiling libc v0.1.8
   Compiling gcc v0.3.5
   Compiling time v0.1.25
   Compiling now v0.1.0 (file:///C:/Users/drk/AppData/Local/Cargo/script-cache/file-now-1410beff463a5c50726f-8dbf2bcf69d2d8208c4c)
Sat, 30 May 2015 19:26:57 +1000
```

If you are in a hurry, the above can also be accomplished by telling `cargo-script` that you wish to evaluate an *expression*, rather than an actual file:

```text
$ cargo script --dep time --expr \
    "extern crate time; time::now().rfc822z().to_string()"
    Updating registry `https://github.com/rust-lang/crates.io-index`
   Compiling gcc v0.3.5
   Compiling libc v0.1.8
   Compiling time v0.1.25
   Compiling expr v0.1.0 (file:///C:/Users/drk/AppData/Local/Cargo/script-cache/expr-a7ffe37fbe6dccff132f)
"Sat, 30 May 2015 19:32:18 +1000"
```

Dependencies can also be specified with specific versions (*e.g.* `--dep time=0.1.25`); when omitted, `cargo-script` will simply use `"*"` for the manifest.  The above can *also* be written variously as:

* `cargo script -d time -e "extern crate time; ..."`
* `cargo script -d time -x time -e "..."`
* `cargo script --dep-extern time --expr "..."`
* `cargo script -D time -e "..."`

The `--dep-extern`/`-D` option can be used to insert an automatic `extern crate` item into an expression (or loop, as shown below) script.  This *only* works when the package name and compiled crate name match.

If you wish to use a dependency where the package and crate names *do not* match, you can specify the dependency with `--dep`/`-d`, and the extern crate name with `--extern`/`-x`.

Finally, you can also use `cargo-script` to write a quick stream filter, by specifying a closure to be called for each line read from stdin, like so:

```text
$ cat now.crs | cargo script --loop \
    "let mut n=0; move |l| {n+=1; println!(\"{:>6}: {}\",n,l.trim_right())}"
   Compiling loop v0.1.0 (file:///C:/Users/drk/AppData/Local/Cargo/script-cache/loop-58079283761aab8433b1)
     1: [dependencies]
     2: time = "0.1.25"
     3: ---
     4: extern crate time;
     5: fn main() {
     6:     println!("{}", time::now().rfc822z());
     7: }
```

Note that you can achieve a similar effect to the above by using the `--count` flag, which causes the line number to be passed as a second argument to your closure:

```text
$ cat now.crs | cargo script --count --loop \
    "|l,n| println!(\"{:>6}: {}\", n, l.trim_right())"
   Compiling loop v0.1.0 (file:///C:/Users/drk/AppData/Local/Cargo/script-cache/loop-58079283761aab8433b1)
     1: [dependencies]
     2: time = "0.1.25"
     3: ---
     4: extern crate time;
     5: fn main() {
     6:     println!("{}", time::now().rfc822z());
     7: }
```

## Things That Should Probably Be Done

* Suppress Cargo/rustc output unless there's actually a problem.  Downside: scripts that require lots of network access and dependency compilation will appear to hang for a while with no feedback.

* Somehow convince the Cargo devs to add aggressive caching of dependencies so that compiling anything that has dependencies doesn't take an age.

* Gist support?  I mean, if it's good enough for playpen...

* Some kind of install script would be nice.
