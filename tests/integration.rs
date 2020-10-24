/*!
# Why is this here?

Because *both* Cargo and Rust both know better than I do and won't let me tell them to stop running the tests in parallel.  This is a problem because they do not, in fact, know better than me: Cargo doesn't do *any* locking, which causes random failures as two tests try to update the registry simultaneously (quite *why* Cargo needs to update the registry so fucking often I have no damn idea).

*All* integration tests have to be glommed into a single runner so that we can use locks to prevent Cargo from falling over and breaking both its legs as soon as a gentle breeze comes along.  I *would* do this "properly" using file locks, except that's apparently impossible in Rust without writing the whole stack yourself directly on native OS calls, and I just can't be arsed to go to *that* much effort just to get some bloody tests to work.
*/
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate scan_rules;
#[macro_use]
mod util;

mod tests {
    mod expr;
    mod script;
    mod version;
}
