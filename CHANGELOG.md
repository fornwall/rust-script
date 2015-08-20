
# v0.1.2

**New:**

* Added `-e` and `-l` as shorthands for `--expr` and `--loop`.

* Added `--dep-extern`/`-D`.  This introduces a dependency and causes an appropriate `#[macro_use] extern crate $name;` item to be added.  This only applies to expression and loop scripts.

* Added `--extern`/`-x`.  This adds a explicit `#[macro_use] extern crate $name` item.  This only applies to expression and loop scripts.

# v0.1.1

**New:**

* Not-Windows support, contributed by @Ryman.

* Added support for two new embedded manifest formats: short comment manifests and code block manifests.  Compared to prefix manifests, these have the advantage of allowing scripts to be valid Rust code, as well as having a measure of self-documentation.

* You can now pass arguments to scripts.  If you want to pass switches, you'll need to add `--` after the script name so `cargo script` knows to stop looking for switches.

* Added the `--clear-cache` flag.  This deletes all cached scripts.  It can be provided by itself or as part of a regular invocation.

* Added the `--pkg-path PATH` flag.  This lets you specify where to create the Cargo package.

* Added the `--gen-pkg-only` flag.  This causes `cargo script` to output a Cargo package, but not attempt to build or run it.

**Changed:**

* Expressions and loop closures are now wrapped in blocks, rather than an expression.  This means you can have multiple statements, link to crates, use things, etc. without having to define a block yourself.

* Expressions now have their output displayed using the `{:?}` format specifier.  This allows more types to work without extra effort, but does make strings a bit ugly.

* Changed to a not-as-slow argument parser.

**Removed:**

* Removed content hashing for scripts.  No longer will even minor changes cause Cargo to go back and re-fetch and re-compile all dependencies!  Scripts are cached based on a hash of their absolute path, instead.  Expressions and loop closures are still cached based on a content hash.
