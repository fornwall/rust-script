use clap::ArgAction;

use crate::build_kind::BuildKind;

#[derive(Debug)]
pub struct Args {
    pub script: Option<String>,
    pub script_args: Vec<String>,
    pub expr: bool,
    pub loop_: bool,
    pub count: bool,
    pub pkg_path: Option<String>,
    pub gen_pkg_only: bool,
    pub cargo_output: bool,
    pub clear_cache: bool,
    pub debug: bool,
    pub dep: Vec<String>,
    pub extern_: Vec<String>,
    pub force: bool,
    pub unstable_features: Vec<String>,
    pub build_kind: BuildKind,
    pub toolchain_version: Option<String>,
    #[cfg(windows)]
    pub install_file_association: bool,
    #[cfg(windows)]
    pub uninstall_file_association: bool,
}

impl Args {
    pub fn parse() -> Self {
        use clap::{Arg, ArgGroup, Command};
        let version = option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
        let about = r#"Compiles and runs a Rust script."#;

        let app = Command::new(crate::consts::PROGRAM_NAME)
            .version(version)
            .about(about)
            .arg(Arg::new("script")
                .index(1)
                .help("Script file or expression to execute.")
                .required_unless_present_any(if cfg!(windows) {
                    ["clear-cache", "install-file-association", "uninstall-file-association"].iter()
                } else {
                    ["clear-cache"].iter()
                })
                .conflicts_with_all(if cfg!(windows) {
                    ["install-file-association", "uninstall-file-association"].iter()
                } else {
                    [].iter()
                })
                .num_args(1..)
                .trailing_var_arg(true)
            )
            .arg(Arg::new("expr")
                .help("Execute <script> as a literal expression and display the result.")
                .long("expr")
                .short('e')
                .action(ArgAction::SetTrue)
                .requires("script")
            )
            .arg(Arg::new("loop")
                .help("Execute <script> as a literal closure once for each line from stdin.")
                .long("loop")
                .short('l')
                .action(ArgAction::SetTrue)
                .requires("script")
            )
            .group(ArgGroup::new("expr_or_loop")
                .args(["expr", "loop"])
            )
            /*
            Options that impact the script being executed.
            */
            .arg(Arg::new("cargo-output")
                .help("Show output from cargo when building.")
                .short('c')
                .long("cargo-output")
                .action(ArgAction::SetTrue)
                .requires("script")
            )
            .arg(Arg::new("count")
                .help("Invoke the loop closure with two arguments: line, and line number.")
                .long("count")
                .action(ArgAction::SetTrue)
                .requires("loop")
            )
            .arg(Arg::new("debug")
                .help("Build a debug executable, not an optimised one.")
                .long("debug")
                .action(ArgAction::SetTrue)
            )
            .arg(Arg::new("dep")
                .help("Add a dependency - either just the package name (for the latest version) or as `name=version`.")
                .long("dep")
                .short('d')
                .num_args(1..)
                .number_of_values(1)
            )
            .arg(Arg::new("extern")
                .help("Adds an `#[macro_use] extern crate name;` item for expressions and loop scripts.")
                .long("extern")
                .short('x')
                .num_args(1..)
                .requires("expr_or_loop")
            )
            .arg(Arg::new("unstable_features")
                .help("Add a #![feature] declaration to the crate.")
                .long("unstable-feature")
                .short('u')
                .num_args(1..)
                .requires("expr_or_loop")
            )

            /*
            Options that change how rust-script itself behaves, and don't alter what the script will do.
            */
            .arg(Arg::new("clear-cache")
                .help("Clears out the script cache.")
                .long("clear-cache")
                .exclusive(true)
                .action(ArgAction::SetTrue),
            )
            .arg(Arg::new("force")
                .help("Force the script to be rebuilt.")
                .long("force")
                .short('f')
                .action(ArgAction::SetTrue)
                .requires("script")
            )
            .arg(Arg::new("gen_pkg_only")
                .help("Generate the Cargo package and print the path to it, but don't compile or run it.")
                .long("package")
                .short('p')
                .action(ArgAction::SetTrue)
                .requires("script")
                .conflicts_with_all(["debug", "force", "test", "bench"])
            )
            .arg(Arg::new("pkg_path")
                .help("Specify where to place the generated Cargo package.")
                .long("pkg-path")
                .num_args(1)
                .requires("script")
                .conflicts_with_all(["clear-cache", "force"])
            )
            .arg(Arg::new("test")
                .help("Compile and run tests.")
                .long("test")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["bench", "debug", "force"])
            )
            .arg(Arg::new("bench")
                .help("Compile and run benchmarks. Requires a nightly toolchain.")
                .long("bench")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["test", "debug", "force"])
            )
            .arg(Arg::new("toolchain")
                .help("Build the script using the given toolchain version.")
                .long("toolchain")
                .short('t')
                .num_args(1)
                // Benchmarking currently requires nightly:
                .conflicts_with("bench")
            );

        #[cfg(windows)]
        let app = app
            .arg(
                Arg::new("install-file-association")
                    .help("Install a file association so that rust-script executes .ers files.")
                    .long("install-file-association")
                    .exclusive(true)
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("uninstall-file-association")
                    .help(
                        "Uninstall the file association that makes rust-script execute .ers files.",
                    )
                    .long("uninstall-file-association")
                    .exclusive(true)
                    .action(ArgAction::SetTrue),
            )
            .group(
                ArgGroup::new("file-association")
                    .args(["install-file-association", "uninstall-file-association"]),
            );

        let mut m = app.get_matches();

        let script_and_args: Option<Vec<String>> = m
            .remove_many::<String>("script")
            .map(|values| values.collect());
        let script;
        let script_args: Vec<String>;
        if let Some(script_and_args) = script_and_args {
            script = script_and_args.first().map(|s| s.to_string());
            script_args = if script_and_args.len() > 1 {
                Vec::from_iter(script_and_args[1..].iter().map(|s| s.to_string()))
            } else {
                Vec::new()
            };
        } else {
            script = None;
            script_args = Vec::new();
        }

        Self {
            script,
            script_args,

            expr: m.get_flag("expr"),
            loop_: m.get_flag("loop"),
            count: m.get_flag("count"),

            pkg_path: m.get_one::<String>("pkg_path").map(Into::into),
            gen_pkg_only: m.get_flag("gen_pkg_only"),
            cargo_output: m.get_flag("cargo-output"),
            clear_cache: m.get_flag("clear-cache"),
            debug: m.get_flag("debug"),
            dep: m
                .remove_many::<String>("dep")
                .map(|values| values.collect())
                .unwrap_or_default(),
            extern_: m
                .remove_many::<String>("extern")
                .map(|values| values.collect())
                .unwrap_or_default(),
            force: m.get_flag("force"),
            unstable_features: m
                .remove_many::<String>("unstable_features")
                .map(|values| values.collect())
                .unwrap_or_default(),
            build_kind: BuildKind::from_flags(m.get_flag("test"), m.get_flag("bench")),
            toolchain_version: m.get_one::<String>("toolchain").map(Into::into),
            #[cfg(windows)]
            install_file_association: m.get_flag("install-file-association"),
            #[cfg(windows)]
            uninstall_file_association: m.get_flag("uninstall-file-association"),
        }
    }
}
