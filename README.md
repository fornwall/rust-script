# `cargo-script`

`cargo-script` is a Cargo subcommand designed to let people quickly and easily run Rust "scripts" which can make use of Cargo's package ecosystem.

Or, to put it in other words, it lets you write useful, but small, Rust programs without having to create a new directory and faff about with `Cargo.toml`.

As such, `cargo-script` does two major things:

1. Given a script, it extracts the embedded Cargo manifest and merges it with some sensible defaults.  This manifest, along with the source code, is written to a fresh Cargo package on-disk.

2. It caches the generated and compiled packages, regenerating them only if the script or its metadata have changed.

## Installation

`cargo-script` requires a nightly build of `rustc` due to the use of several unstable features.  Aside from that, it should build cleanly with `cargo`.

Once built, you should place the resulting executable somewhere on your `PATH`.  At that point, you should be able to invoke it by using `cargo script`.

Note that you *can* run the executable directly, but the first argument will *need* to be `script`.

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

You may also embed a partial Cargo manifest at the start of your script, as shown below.  `cargo-script` specifically supports the `.crs` extension to distinguish such files from regular Rust source, but it will process regular `.rs` files in *exactly* the same manner.

`now.crs`:

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

The partial manifest is terminated by a line consisting entirely of whitespace and *at least* three hyphens.  `cargo-script` will also end the manifest if it encounters anything that looks suscpiciously like Rust code, but this should not be relied upon; such detection is *extremely* hacky.

If you are in a hurry, the above can also be accomplished by telling `cargo-script` that you wish to evaluate an *expression*, rather than an actual file:

```shell
$ cargo script --dep time --expr "{extern crate time; time::now().rfc822z()}"
    Updating registry `https://github.com/rust-lang/crates.io-index`
   Compiling gcc v0.3.5
   Compiling libc v0.1.8
   Compiling time v0.1.25
   Compiling expr v0.1.0 (file:///C:/Users/drk/AppData/Local/Cargo/script-cache/expr-a7ffe37fbe6dccff132f)
Sat, 30 May 2015 19:32:18 +1000
```

Dependencies can also be specified with specific versions (*e.g.* `--dep time=0.1.25`); when omitted, `cargo-script` will simply use `"*"` for the manifest.

Finally, you can also use `cargo-script` to write a quick stream filter, by specifying a closure to be called for each line read from stdin, like so:

```shell
$ cat now.crs | cargo script --count --loop \
    '|l,n| println!("{:>6}: {}", n, l.trim_right())'
   Compiling loop v0.1.0 (file:///C:/Users/drk/AppData/Local/Cargo/script-cache/loop-58079283761aab8433b1)
     1: [dependencies]
     2: time = "0.1.25"
     3: ---
     4: extern crate time;
     5: fn main() {
     6:     println!("{}", time::now().rfc822z());
     7: }
```

Without the `--count` argument, only the contents of each line is passed to your closure.  No, there is no easy way to create state that is captured from outside the closure; sorry.

## Things That Should Probably Be Done

* `not(windows)` port; see the `platform` module.

* Actually clean up the cache directory, rather than flooding it with megabytes of code and executables.

* *Definitely* clean up after a failed compilation.

* Somehow convince the Cargo devs to add aggressive caching of dependencies so that compiling anything that has dependencies doesn't take an age.

* *Maybe* don't cache based on content; currently, it means that *any* change to a script or expression causes Cargo to re-download and re-compile all dependencies which is *bloody miserable*.

* ...that, or add some sort of `--no-cache` flag that shoves everything into a single folder.

* Gist support?  I mean, if it's good enough for playpen...
