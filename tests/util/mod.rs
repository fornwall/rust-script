use std::sync::Mutex;

macro_rules! rust_script {
    (
        #[env($($env_k:ident=$env_v:expr),* $(,)*)]
        $($args:expr),* $(,)*
    ) => {
        {
            extern crate tempfile;
            use std::process::Command;

            let cargo_lock = crate::util::CARGO_MUTEX.lock().expect("Could not acquire Cargo mutex");

            let cmd_str;
            let out = {
                let target_dir = ::std::env::var("CARGO_TARGET_DIR")
                    .unwrap_or_else(|_| String::from("target"));
                let mut cmd = Command::new(format!("{}/debug/rust-script", target_dir));
                $(
                    cmd.arg($args);
                )*

                cmd.env_remove("CARGO_TARGET_DIR");
                $(cmd.env(stringify!($env_k), $env_v);)*

                cmd_str = format!("{:?}", cmd);

                cmd.output()
                    .map(crate::util::Output::from)
            };

            if let Ok(out) = out.as_ref() {
                println!("rust-script cmd: {}", cmd_str);
                println!("rust-script stdout:");
                println!("-----");
                println!("{}", out.stdout);
                println!("-----");
                println!("rust-script stderr:");
                println!("-----");
                println!("{}", out.stderr);
                println!("-----");
            }

            drop(cargo_lock);

            out
        }
    };

    ($($args:expr),* $(,)*) => {
        rust_script!(#[env()] $($args),*)
    };
}

macro_rules! with_output_marker {
    (prelude $p:expr; $e:expr) => {
        format!(concat!($p, "{}", $e), crate::util::OUTPUT_MARKER_CODE)
    };

    ($e:expr) => {
        format!(concat!("{}", $e), crate::util::OUTPUT_MARKER_CODE)
    };
}

lazy_static! {
    #[doc(hidden)]
    pub static ref CARGO_MUTEX: Mutex<()> = Mutex::new(());
}

pub const OUTPUT_MARKER: &str = "--output--";
pub const OUTPUT_MARKER_CODE: &str = "println!(\"--output--\");";

pub struct Output {
    pub status: ::std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn stdout_output(&self) -> &str {
        assert!(self.success());
        for marker in self.stdout.matches(OUTPUT_MARKER) {
            let i = subslice_offset(&self.stdout, marker).expect("couldn't find marker in output");
            let before_cp = self.stdout[..i].chars().next_back().unwrap_or('\n');
            if !(before_cp == '\r' || before_cp == '\n') {
                continue;
            }
            let after = &self.stdout[i + OUTPUT_MARKER.len()..];
            let after_cp = after.chars().next().expect("couldn't find cp after marker");
            if !(after_cp == '\r' || after_cp == '\n') {
                continue;
            }
            return after;
        }
        panic!("could not find `{}` in script output", OUTPUT_MARKER);
    }

    pub fn success(&self) -> bool {
        self.status.success()
    }
}

impl From<::std::process::Output> for Output {
    fn from(v: ::std::process::Output) -> Self {
        Self {
            status: v.status,
            stdout: String::from_utf8(v.stdout).unwrap(),
            stderr: String::from_utf8(v.stderr).unwrap(),
        }
    }
}

fn subslice_offset(outer: &str, inner: &str) -> Option<usize> {
    let outer_beg = outer.as_ptr() as usize;
    let inner = inner.as_ptr() as usize;
    if inner < outer_beg || inner > outer_beg.wrapping_add(outer.len()) {
        None
    } else {
        Some(inner.wrapping_sub(outer_beg))
    }
}
