/*
Copyright ⓒ 2017 cargo-script contributors.

Licensed under the MIT license (see LICENSE or <http://opensource.org
/licenses/MIT>) or the Apache License, Version 2.0 (see LICENSE of
<http://www.apache.org/licenses/LICENSE-2.0>), at your option. All
files in the project carrying such notice may not be copied, modified,
or distributed except according to those terms.
*/
extern crate rustc_version;
use rustc_version::{version_matches};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    /*
    Environment might suffer from <https://github.com/DanielKeep/cargo-script/issues/50>.
    */
    if cfg!(windows) {
        println!("cargo:rustc-cfg=issue_50");
    }

    /*
    With 1.15, linking on Windows was changed in regards to when it emits `dllimport`.  This means that the *old* code for linking to `FOLDERID_LocalAppData` no longer works.  Unfortunately, it *also* means that the *new* code doesn't work prior to 1.15.

    This controls which linking behaviour we need to work with.
    */
    if version_matches("<1.15.0") {
        println!("cargo:rustc-cfg=old_rustc_windows_linking_behaviour");
    }

    /*
    Before 1.13, there was no `?` operator. One of the tests needs this information.
    */
    if version_matches(">=1.13.0") {
        println!("cargo:rustc-cfg=has_qmark");
    }
}
