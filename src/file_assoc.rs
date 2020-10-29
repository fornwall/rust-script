/*!
This module deals with setting up file associations.

Since this only makes sense on Windows, this entire module is Windows-only.
*/
#![cfg(windows)]
use self::itertools::Itertools;
use crate::error::{Blame, Result};
use std::io;

pub fn install_file_association() -> Result<()> {
    use self::winreg::enums as wre;
    use self::winreg::RegKey;
    use std::env;

    let cs_path = env::current_exe()?;
    let cs_path = cs_path.canonicalize()?;
    let rcs_path = cs_path.with_file_name("rust-script.exe");

    if !rcs_path.exists() {
        return Err((Blame::Human, format!("{:?} not found", rcs_path)).into());
    }

    // We have to remove the `\\?\` prefix because, if we don't, the shell freaks out.
    let rcs_path = rcs_path.to_string_lossy();
    let rcs_path = if rcs_path.starts_with(r#"\\?\"#) {
        &rcs_path[4..]
    } else {
        &rcs_path[..]
    };

    let res = (|| -> io::Result<()> {
        let hlcr = RegKey::predef(wre::HKEY_CLASSES_ROOT);
        let (dot_ers, _) = hlcr.create_subkey(".ers")?;
        dot_ers.set_value("", &"RustScript.Ers")?;

        let (cs_ers, _) = hlcr.create_subkey("RustScript.Ers")?;
        cs_ers.set_value("", &"Rust Script")?;

        let (sh_o_c, _) = cs_ers.create_subkey(r#"shell\open\command"#)?;
        sh_o_c.set_value("", &format!(r#""{}" "%1" %*"#, rcs_path))?;
        Ok(())
    })();

    match res {
        Ok(()) => (),
        Err(e) => {
            if e.kind() == io::ErrorKind::PermissionDenied {
                println!(
                    "Access denied.  Make sure you run this command from an administrator prompt."
                );
                return Err((Blame::Human, e).into());
            } else {
                return Err(e.into());
            }
        }
    }

    println!("Created rust-script registry entry.");
    println!("- Handler set to: {}", rcs_path);

    let hklm = RegKey::predef(wre::HKEY_LOCAL_MACHINE);
    let env =
        hklm.open_subkey(r#"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"#)?;

    let pathext: String = env.get_value("PATHEXT")?;
    if !pathext.split(';').any(|e| e.eq_ignore_ascii_case(".ers")) {
        let pathext = pathext.split(';').chain(Some(".ERS")).join(";");
        env.set_value("PATHEXT", &pathext)?;
    }

    println!("Added `.ers` to PATHEXT.  You may need to log out for the change to take effect.");

    Ok(())
}

pub fn uninstall_file_association() -> Result<()> {
    use self::winreg::enums as wre;
    use self::winreg::RegKey;

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

    {
        let hklm = RegKey::predef(wre::HKEY_LOCAL_MACHINE);
        let env =
            hklm.open_subkey(r#"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"#)?;

        let pathext: String = env.get_value("PATHEXT")?;
        if pathext.split(';').any(|e| e.eq_ignore_ascii_case(".ers")) {
            let pathext = pathext
                .split(';')
                .filter(|e| !e.eq_ignore_ascii_case(".ers"))
                .join(";");
            env.set_value("PATHEXT", &pathext)?;
            println!("Removed `.ers` from PATHEXT.  You may need to log out for the change to take effect.");
        }
    }

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
