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
    // This is a String instead of an
    // enum since one can have custom
    // toolchains (ex. a rustc developer
    // will probably have `stage1`).
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
        .trailing_var_arg(true)
            .arg(Arg::new("script")
                .index(1)
                .help("Script file or expression to execute.")
                .required_unless_present_any(if cfg!(windows) {
                    vec!["clear-cache", "install-file-association", "uninstall-file-association"]
                } else {
                    vec!["clear-cache"]
                })
                .conflicts_with_all(if cfg!(windows) {
                    &["install-file-association", "uninstall-file-association"]
                } else {
                    &[]
                })
                .multiple_values(true)
            )
            .arg(Arg::new("expr")
                .help("Execute <script> as a literal expression and display the result.")
                .long("expr")
                .short('e')
                .takes_value(false)
                .requires("script")
            )
            .arg(Arg::new("loop")
                .help("Execute <script> as a literal closure once for each line from stdin.")
                .long("loop")
                .short('l')
                .takes_value(false)
                .requires("script")
            )
            .group(ArgGroup::new("expr_or_loop")
                .args(&["expr", "loop"])
            )
            /*
            Options that impact the script being executed.
            */
            .arg(Arg::new("cargo-output")
                .help("Show output from cargo when building.")
                .short('o')
                .long("cargo-output")
                .requires("script")
            )
            .arg(Arg::new("count")
                .help("Invoke the loop closure with two arguments: line, and line number.")
                .long("count")
                .requires("loop")
            )
            .arg(Arg::new("debug")
                .help("Build a debug executable, not an optimised one.")
                .long("debug")
            )
            .arg(Arg::new("dep")
                .help("Add an additional Cargo dependency. Each SPEC can be either just the package name (which will assume the latest version) or a full `name=version` spec.")
                .long("dep")
                .short('d')
                .takes_value(true)
                .multiple_occurrences(true)
                .number_of_values(1)
            )
            .arg(Arg::new("extern")
                .help("Adds an `#[macro_use] extern crate name;` item for expressions and loop scripts.")
                .long("extern")
                .short('x')
                .takes_value(true)
                .multiple_occurrences(true)
                .requires("expr_or_loop")
            )
            .arg(Arg::new("unstable_features")
                .help("Add a #![feature] declaration to the crate.")
                .long("unstable-feature")
                .short('u')
                .takes_value(true)
                .multiple_occurrences(true)
                .requires("expr_or_loop")
            )

            /*
            Options that change how rust-script itself behaves, and don't alter what the script will do.
            */
            .arg(Arg::new("clear-cache")
                .help("Clears out the script cache.")
                .long("clear-cache")
            )
            .arg(Arg::new("force")
                .help("Force the script to be rebuilt.")
                .long("force")
                .requires("script")
            )
            .arg(Arg::new("gen_pkg_only")
                .help("Generate the Cargo package, but don't compile or run it.")
                .long("gen-pkg-only")
                .requires("script")
                .conflicts_with_all(&["debug", "force", "test", "bench"])
            )
            .arg(Arg::new("pkg_path")
                .help("Specify where to place the generated Cargo package.")
                .long("pkg-path")
                .takes_value(true)
                .requires("script")
                .conflicts_with_all(&["clear-cache", "force"])
            )
            .arg(Arg::new("test")
                .help("Compile and run tests.")
                .long("test")
                .conflicts_with_all(&["bench", "debug", "force"])
            )
            .arg(Arg::new("bench")
                .help("Compile and run benchmarks. Requires a nightly toolchain.")
                .long("bench")
                .conflicts_with_all(&["test", "debug", "force"])
            )
            .arg(Arg::new("toolchain-version")
                .help("Build the script using the given toolchain version.")
                .long("toolchain-version")
                // "channel"
                .short('c')
                .takes_value(true)
                // FIXME: remove if benchmarking is stabilized
                .conflicts_with("bench")
            );

        #[cfg(windows)]
        let app = app
            .arg(
                Arg::new("install-file-association")
                    .help("Install a file association so that rust-script executes .ers files.")
                    .long("install-file-association"),
            )
            .arg(
                Arg::new("uninstall-file-association")
                    .help(
                        "Uninstall the file association that makes rust-script execute .ers files.",
                    )
                    .long("uninstall-file-association"),
            )
            .group(
                ArgGroup::new("file-association")
                    .args(&["install-file-association", "uninstall-file-association"]),
            );

        let m = app.get_matches();

        fn owned_vec_string<'a, I>(v: Option<I>) -> Vec<String>
        where
            I: ::std::iter::Iterator<Item = &'a str>,
        {
            v.map(|itr| itr.map(Into::into).collect())
                .unwrap_or_default()
        }

        let script_and_args: Option<Vec<&str>> = m.values_of("script").map(|o| o.collect());
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

            expr: m.is_present("expr"),
            loop_: m.is_present("loop"),
            count: m.is_present("count"),

            pkg_path: m.value_of("pkg_path").map(Into::into),
            gen_pkg_only: m.is_present("gen_pkg_only"),
            cargo_output: m.is_present("cargo-output"),
            clear_cache: m.is_present("clear-cache"),
            debug: m.is_present("debug"),
            dep: owned_vec_string(m.values_of("dep")),
            extern_: owned_vec_string(m.values_of("extern")),
            force: m.is_present("force"),
            unstable_features: owned_vec_string(m.values_of("unstable_features")),
            build_kind: BuildKind::from_flags(m.is_present("test"), m.is_present("bench")),
            toolchain_version: m.value_of("toolchain-version").map(Into::into),
            #[cfg(windows)]
            install_file_association: m.is_present("install-file-association"),
            #[cfg(windows)]
            uninstall_file_association: m.is_present("uninstall-file-association"),
        }
    }
}
