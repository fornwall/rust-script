/*!
This module is for platform-specific stuff.
*/

pub use self::inner::{force_cargo_color, read_path, write_path};

use crate::consts;
use crate::error::{Blame, MainError};
use std::fs;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// Last-modified time of a file, in milliseconds since the UNIX epoch.
pub fn file_last_modified(file: &fs::File) -> u128 {
    file.metadata()
        .and_then(|md| {
            md.modified()
                .map(|t| t.duration_since(UNIX_EPOCH).unwrap().as_millis())
        })
        .unwrap_or(0)
}

// Current system time, in milliseconds since the UNIX epoch.
pub fn current_time() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

pub fn cache_dir() -> Result<PathBuf, MainError> {
    dirs_next::cache_dir()
        .map(|dir| dir.join(consts::PROGRAM_NAME))
        .ok_or_else(|| (Blame::Human, "Cannot get cache directory").into())
}

pub fn script_cache_path() -> Result<PathBuf, MainError> {
    cache_dir().map(|dir| dir.join("scripts"))
}

pub fn binary_cache_path() -> Result<PathBuf, MainError> {
    cache_dir().map(|dir| dir.join("binaries"))
}

pub fn templates_dir() -> Result<PathBuf, MainError> {
    if cfg!(debug_assertions) {
        if let Ok(path) = std::env::var("RUST_SCRIPT_DEBUG_TEMPLATE_PATH") {
            return Ok(path.into());
        }
    }

    dirs_next::data_local_dir()
        .map(|dir| dir.join(consts::PROGRAM_NAME).join("templates"))
        .ok_or_else(|| (Blame::Human, "Cannot get cache directory").into())
}

#[cfg(unix)]
mod inner {
    extern crate atty;

    pub use super::*;

    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    pub fn write_path<W>(w: &mut W, path: &Path) -> io::Result<()>
    where
        W: io::Write,
    {
        w.write_all(path.as_os_str().as_bytes())
    }

    pub fn read_path<R>(r: &mut R) -> io::Result<PathBuf>
    where
        R: io::Read,
    {
        use std::ffi::OsStr;
        let mut buf = vec![];
        r.read_to_end(&mut buf)?;
        Ok(OsStr::from_bytes(&buf).into())
    }

    /**
    Returns `true` if `rust-script` should force Cargo to use coloured output.

    This depends on whether `rust-script`'s STDERR is connected to a TTY or not.
    */
    pub fn force_cargo_color() -> bool {
        atty::is(atty::Stream::Stderr)
    }
}

#[cfg(windows)]
pub mod inner {
    #![allow(non_snake_case)]

    extern crate ole32;
    extern crate shell32;
    extern crate winapi;

    pub use super::*;

    use std::ffi::OsString;
    use std::fmt;

    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    struct WinError(winapi::HRESULT);

    impl fmt::Display for WinError {
        fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
            write!(fmt, "HRESULT({})", self.0)
        }
    }

    pub fn write_path<W>(w: &mut W, path: &Path) -> io::Result<()>
    where
        W: io::Write,
    {
        for word in path.as_os_str().encode_wide() {
            let lo = (word & 0xff) as u8;
            let hi = (word >> 8) as u8;
            w.write_all(&[lo, hi])?;
        }
        Ok(())
    }

    pub fn read_path<R>(r: &mut R) -> io::Result<PathBuf>
    where
        R: io::Read,
    {
        let mut buf = vec![];
        r.read_to_end(&mut buf)?;

        let mut words = Vec::with_capacity(buf.len() / 2);
        let mut it = buf.iter().cloned();
        while let Some(lo) = it.next() {
            let hi = it.next().unwrap();
            words.push(lo as u16 | ((hi as u16) << 8));
        }

        Ok(OsString::from_wide(&words).into())
    }

    /**
    Returns `true` if `rust-script` should force Cargo to use coloured output.

    Always returns `false` on Windows because colour is communicated over a side-channel.
    */
    pub fn force_cargo_color() -> bool {
        false
    }
}
