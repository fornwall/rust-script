#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! libc = { version = "0.2", default-features = false }
//!
//! [profile.release]
//! strip = true
//! lto = true
//! opt-level = "s" # "z"
//! codegen-units = 1
//! panic = "abort"
//! ```

#![no_std]
#![no_main]

#[panic_handler]
fn my_panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn main(_argc: isize, _argv: *const *const u8) -> isize {
    unsafe {
        libc::printf("--output--\n\0".as_ptr() as *const _);
        libc::printf("hello, world\n\0".as_ptr() as *const _);
    }
    0
}
