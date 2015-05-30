/*!
This module is for platform-specific stuff.
*/

pub use self::inner::get_cache_dir_for;

#[cfg(windows)]
pub mod inner {
    #![allow(non_snake_case)]

    extern crate ole32;
    extern crate shell32;
    extern crate winapi;
    extern crate uuid;

    use std::ffi::OsString;
    use std::fmt;
    use std::path::{Path, PathBuf};
    use std::mem;
    use std::os::windows::ffi::OsStringExt;
    use self::uuid::FOLDERID_LocalAppData;
    use error::MainError;

    /**
    Get a directory suitable for storing user- and machine-specific data which may or may not be persisted across sessions.

    On Windows, LocalAppData is where user- and machine- specific data should go, but it *might* be more appropriate to use whatever the official name for "Program Data" is, though.
    */
    pub fn get_cache_dir_for<P>(product: P) -> Result<PathBuf, MainError>
    where P: AsRef<Path> {
        let dir = try!(SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, ::std::ptr::null_mut())
            .map_err(|e| e.to_string()));
        Ok(Path::new(&dir).to_path_buf().join(product))
    }

    type WinResult<T> = Result<T, WinError>;

    struct WinError(winapi::HRESULT);

    impl fmt::Display for WinError {
        fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
            write!(fmt, "HRESULT({})", self.0)
        }
    }

    // impl From<WinError> for MainError {
    //     fn from(v: WinError) -> MainError {
    //         v.to_string().into()
    //     }
    // }

    fn SHGetKnownFolderPath(rfid: &winapi::KNOWNFOLDERID, dwFlags: winapi::DWORD, hToken: winapi::HANDLE) -> WinResult<OsString> {
        use self::winapi::PWSTR;
        let mut psz_path: PWSTR = unsafe { mem::uninitialized() };
        let hresult = unsafe {
            shell32::SHGetKnownFolderPath(
                rfid,
                dwFlags,
                hToken,
                mem::transmute(&mut psz_path as &mut PWSTR as *mut PWSTR)
            )
        };

        if hresult == winapi::S_OK {
            let r = unsafe { pwstr_to_os_string(psz_path) };
            unsafe { ole32::CoTaskMemFree(psz_path as *mut _) };
            Ok(r)
        } else {
            Err(WinError(hresult))
        }
    }

    unsafe fn pwstr_to_os_string(ptr: winapi::PWSTR) -> OsString {
        OsStringExt::from_wide(::std::slice::from_raw_parts(ptr, pwstr_len(ptr)))
    }

    unsafe fn pwstr_len(mut ptr: winapi::PWSTR) -> usize {
        let mut len = 0;
        while *ptr != 0 {
            len += 1;
            ptr = ptr.offset(1);
        }
        len
    }
}
