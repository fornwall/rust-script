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
    With 1.15, linking on Windows was changed in regards to when it emits `dllimport`.  This means that the *old* code for linking to `FOLDERID_LocalAppData` no longer works.  Unfortunately, it *also* means that the *new* code doesn't work prior to 1.15.

    This controls which linking behaviour we need to work with.
    */
    if version_matches("<1.15.0") {
        println!("cargo:rustc-cfg=old_rustc_windows_linking_behaviour");
    }
}
