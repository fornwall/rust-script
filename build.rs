/*
Copyright ⓒ 2017 cargo-script contributors.

Licensed under the MIT license (see LICENSE or <http://opensource.org
/licenses/MIT>) or the Apache License, Version 2.0 (see LICENSE of
<http://www.apache.org/licenses/LICENSE-2.0>), at your option. All
files in the project carrying such notice may not be copied, modified,
or distributed except according to those terms.
*/

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    /*
    Environment might suffer from <https://github.com/DanielKeep/cargo-script/issues/50>.
    */
    if cfg!(windows) {
        println!("cargo:rustc-cfg=issue_50");
    }
}
