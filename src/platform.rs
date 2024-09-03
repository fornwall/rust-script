/*!
This module is for platform-specific stuff.
*/

pub use self::inner::force_cargo_color;

use std::fs;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// Last-modified time of a directory, in milliseconds since the UNIX epoch.
pub fn dir_last_modified(dir: &fs::DirEntry) -> u128 {
    dir.metadata()
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

pub fn cache_dir() -> PathBuf {
    #[cfg(not(test))]
    {
        dirs::cache_dir()
            .map(|dir| dir.join(crate::consts::PROGRAM_NAME))
            .expect("Cannot get cache directory")
    }
    #[cfg(test)]
    {
        use lazy_static::lazy_static;
        lazy_static! {
            static ref TEMP_DIR: tempfile::TempDir = tempfile::TempDir::new().unwrap();
        }
        TEMP_DIR.path().to_path_buf()
    }
}

pub fn generated_projects_cache_path() -> PathBuf {
    cache_dir().join("projects")
}

pub fn binary_cache_path() -> PathBuf {
    cache_dir().join("binaries")
}

#[cfg(unix)]
mod inner {
    use is_terminal::IsTerminal as _;

    /**
    Returns `true` if `rust-script` should force Cargo to use coloured output.

    This depends on whether `rust-script`'s STDERR is connected to a TTY or not.
    */
    pub fn force_cargo_color() -> bool {
        std::io::stderr().is_terminal()
    }
}

#[cfg(windows)]
pub mod inner {
    /**
    Returns `true` if `rust-script` should force Cargo to use coloured output.

    Always returns `false` on Windows because colour is communicated over a side-channel.
    */
    pub fn force_cargo_color() -> bool {
        false
    }
}
