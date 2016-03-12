use std::sync::Mutex;

macro_rules! cargo_script {
    ($($args:expr),* $(,)*) => {
        {
            extern crate tempdir;
            use std::process::Command;

            let cargo_lock = ::util::CARGO_MUTEX.lock().expect("could not acquire Cargo mutext");

            let temp_dir = tempdir::TempDir::new("cargo-script-test").unwrap();
            let cmd_str;
            let out = {
                let mut cmd = Command::new("target/debug/cargo-script");
                cmd.arg("script");
                cmd.arg("--pkg-path").arg(temp_dir.path());
                $(
                    cmd.arg($args);
                )*

                cmd_str = format!("{:?}", cmd);

                cmd.output()
                    .map(::util::Output::from)
            };

            if let Ok(out) = out.as_ref() {
                println!("cargo-script cmd: {}", cmd_str);
                println!("cargo-script stdout:");
                println!("-----");
                println!("{}", out.stdout);
                println!("-----");
                println!("cargo-script stderr:");
                println!("-----");
                println!("{}", out.stderr);
                println!("-----");
            }

            drop(temp_dir);
            drop(cargo_lock);

            out
        }
    };
}

macro_rules! with_output_marker {
    (prelude $p:expr; $e:expr) => {
        format!(concat!($p, "{}", $e), ::util::OUTPUT_MARKER_CODE)
    };

    ($e:expr) => {
        format!(concat!("{}", $e), ::util::OUTPUT_MARKER_CODE)
    };
}

lazy_static! {
    #[doc(hidden)]
    pub static ref CARGO_MUTEX: Mutex<()> = Mutex::new(());
}

pub const OUTPUT_MARKER: &'static str = "--output--";
pub const OUTPUT_MARKER_CODE: &'static str = "println!(\"--output--\");";

pub struct Output {
    pub status: ::std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn stdout_output(&self) -> &str {
        assert!(self.success());
        for (i, _) in self.stdout.match_indices(OUTPUT_MARKER) {
            let before_cp = self.stdout[..i].chars().rev().next().unwrap();
            if !(before_cp == '\r' || before_cp == '\n') { continue; }
            let after = &self.stdout[i+OUTPUT_MARKER.len()..];
            let after_cp = after.chars().next().unwrap();
            if !(after_cp == '\r' || after_cp == '\n') { continue; }
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
        Output {
            status: v.status,
            stdout: String::from_utf8(v.stdout).unwrap(),
            stderr: String::from_utf8(v.stderr).unwrap(),
        }
    }
}
