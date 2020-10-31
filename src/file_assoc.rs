/*!
This module deals with setting up file associations on Windows
*/
use crate::error::MainResult;
use std::env;
use std::io;
use winreg::{enums as wre, RegKey};

pub fn install_file_association() -> MainResult<()> {
    let rust_script_path = env::current_exe()?.canonicalize()?;
    if !rust_script_path.exists() {
        return Err(format!("{:?} not found", rust_script_path).into());
    }

    // We have to remove the `\\?\` prefix because, if we don't, the shell freaks out.
    let rust_script_path = rust_script_path.to_string_lossy();
    let rust_script_path = if rust_script_path.starts_with(r#"\\?\"#) {
        &rust_script_path[4..]
    } else {
        &rust_script_path[..]
    };

    let res = (|| -> io::Result<()> {
        let hlcr = RegKey::predef(wre::HKEY_CLASSES_ROOT);
        let (dot_ers, _) = hlcr.create_subkey(".ers")?;
        dot_ers.set_value("", &"RustScript.Ers")?;

        let (cs_ers, _) = hlcr.create_subkey("RustScript.Ers")?;
        cs_ers.set_value("", &"Rust Script")?;

        let (sh_o_c, _) = cs_ers.create_subkey(r#"shell\open\command"#)?;
        sh_o_c.set_value("", &format!(r#""{}" "%1" %*"#, rust_script_path))?;
        Ok(())
    })();

    match res {
        Ok(()) => (),
        Err(e) => {
            if e.kind() == io::ErrorKind::PermissionDenied {
                println!(
                    "Access denied.  Make sure you run this command from an administrator prompt."
                );
            }
            return Err(e.into());
        }
    }

    println!("Created rust-script registry entry.");
    println!("- Handler set to: {}", rust_script_path);

    Ok(())
}

pub fn uninstall_file_association() -> MainResult<()> {
    let mut ignored_missing = false;
    {
        let mut notify = || ignored_missing = true;

        let hlcr = RegKey::predef(wre::HKEY_CLASSES_ROOT);
        hlcr.delete_subkey(r#"RustScript.Ers\shell\open\command"#)
            .ignore_missing_and(&mut notify)?;
        hlcr.delete_subkey(r#"RustScript.Ers\shell\open"#)
            .ignore_missing_and(&mut notify)?;
        hlcr.delete_subkey(r#"RustScript.Ers\shell"#)
            .ignore_missing_and(&mut notify)?;
        hlcr.delete_subkey(r#"RustScript.Ers"#)
            .ignore_missing_and(&mut notify)?;
    }

    if ignored_missing {
        println!("Ignored some missing registry entries.");
    }
    println!("Deleted rust-script registry entry.");

    Ok(())
}

trait IgnoreMissing {
    fn ignore_missing_and<F>(self, f: F) -> Self
    where
        F: FnOnce();
}

impl IgnoreMissing for io::Result<()> {
    fn ignore_missing_and<F>(self, f: F) -> Self
    where
        F: FnOnce(),
    {
        match self {
            Ok(()) => Ok(()),
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    f();
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }
}
