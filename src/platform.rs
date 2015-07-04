/*!
This module is for platform-specific stuff.
*/

pub use self::inner::{current_time, file_last_modified, get_cache_dir_for};

#[cfg(any(unix, windows))]
mod inner_unix_or_windows {
    extern crate time;

    /**
    Gets the current system time, in milliseconds since the UNIX epoch.
    */
    pub fn current_time() -> u64 {
        /*
        This is kinda dicey, since *ideally* both this function and `file_last_modified` would be using the same underlying APIs.  They are not, insofar as I know.

        At least, not when targetting Windows.

        That said, so long as everything is in the same units and uses the same epoch, it should be fine.
        */
        let now_1970_utc = time::now_utc().to_timespec();
        if now_1970_utc.sec < 0 || now_1970_utc.nsec < 0 {
            // Fuck it.
            return 0
        }
        let now_ms_1970_utc = (now_1970_utc.sec as u64 * 1000)
            + (now_1970_utc.nsec as u64 / 1_000_000);
        now_ms_1970_utc
    }
}

#[cfg(unix)]
mod inner {
    pub use super::inner_unix_or_windows::current_time;

    use std::path::{Path, PathBuf};
    use std::{cmp, env, fs};
    use std::os::unix::fs::MetadataExt;
    use error::{MainError, Blame};

    /**
    Gets the last-modified time of a file, in milliseconds since the UNIX epoch.
    */
    pub fn file_last_modified(file: &fs::File) -> u64 {
        let mtime_s_1970_utc = file.metadata()
            .map(|md| md.mtime())
            .unwrap_or(0);

        let mtime_s_1970_utc = cmp::max(0, mtime_s_1970_utc);
        mtime_s_1970_utc as u64 * 1000
    }

    /**
    Get a directory suitable for storing user- and machine-specific data which may or may not be persisted across sessions.

    This is chosen to match the location where Cargo places its cache data.
    */
    pub fn get_cache_dir_for<P>(product: P) -> Result<PathBuf, MainError>
    where P: AsRef<Path> {
        let home = match env::var_os("HOME") {
            Some(val) => val,
            None => return Err((Blame::Human, "$HOME is not defined").into())
        };

        match product.as_ref().to_str() {
            Some(s) => {
                let folder = format!(".{}", s.to_lowercase());
                Ok(Path::new(&home).join(folder))
            },
            None => Err("product for `get_cache_dir_for` was not utf8".into())
        }
    }
}

#[cfg(windows)]
pub mod inner {
    #![allow(non_snake_case)]

    extern crate ole32;
    extern crate shell32;
    extern crate winapi;
    extern crate uuid;

    pub use super::inner_unix_or_windows::current_time;

    use std::ffi::OsString;
    use std::fmt;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::mem;
    use std::os::windows::ffi::OsStringExt;
    use self::uuid::FOLDERID_LocalAppData;
    use error::MainError;

    /**
    Gets the last-modified time of a file, in milliseconds since the UNIX epoch.
    */
    pub fn file_last_modified(file: &fs::File) -> u64 {
        use ::std::os::windows::fs::MetadataExt;

        const MS_BETWEEN_1601_1970: u64 = 11_644_473_600_000;

        let mtime_100ns_1601_utc = file.metadata()
            .map(|md| md.last_write_time())
            .unwrap_or(0);
        let mtime_ms_1601_utc = mtime_100ns_1601_utc / (1000*10);

        // This can obviously underflow... but since files created prior to 1970 are going to be *somewhat rare*, I'm just going to saturate to zero.
        let mtime_ms_1970_utc = mtime_ms_1601_utc.saturating_sub(MS_BETWEEN_1601_1970);
        mtime_ms_1970_utc
    }

    /**
    Get a directory suitable for storing user- and machine-specific data which may or may not be persisted across sessions.

    This is *not* chosen to match the location where Cargo places its cache data, because Cargo is *wrong*.  This is at least *less wrong*.

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
