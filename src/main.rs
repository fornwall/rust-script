#![forbid(unsafe_code)]
extern crate env_logger;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate crossbeam_channel;

/**
If this is set to `true`, the digests used for package IDs will be replaced with "stub" to make testing a bit easier.  Obviously, you don't want this `true` for release...
*/
const STUB_HASHES: bool = false;

/**
If this is set to `false`, then code that automatically deletes stuff *won't*.
*/
const ALLOW_AUTO_REMOVE: bool = true;

mod consts;
mod error;
mod manifest;
mod platform;
mod templates;
mod util;

#[cfg(windows)]
mod file_assoc;

#[cfg(not(windows))]
mod file_assoc {}

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use crate::error::{MainError, MainResult};
use crate::util::Defer;
use sha1::{Digest, Sha1};

#[derive(Debug)]
struct Args {
    script: Option<String>,
    script_args: Vec<String>,
    features: Option<String>,

    expr: bool,
    loop_: bool,
    count: bool,

    pkg_path: Option<String>,
    gen_pkg_only: bool,
    build_only: bool,
    cargo_output: bool,
    clear_cache: bool,
    debug: bool,
    dep: Vec<String>,
    extern_: Vec<String>,
    force: bool,
    unstable_features: Vec<String>,
    build_kind: BuildKind,
    template: Option<String>,
    list_templates: bool,

    #[cfg(windows)]
    install_file_association: bool,
    #[cfg(windows)]
    uninstall_file_association: bool,
}

#[derive(Copy, Clone, Debug)]
enum BuildKind {
    Normal,
    Test,
    Bench,
}

impl BuildKind {
    const fn can_exec_directly(&self) -> bool {
        match *self {
            Self::Normal => true,
            Self::Test | Self::Bench => false,
        }
    }

    fn exec_command(&self) -> &'static str {
        match *self {
            Self::Normal => panic!("asked for exec command for normal build"),
            Self::Test => "test",
            Self::Bench => "bench",
        }
    }

    fn from_flags(test: bool, bench: bool) -> Self {
        match (test, bench) {
            (false, false) => Self::Normal,
            (true, false) => Self::Test,
            (false, true) => Self::Bench,
            _ => panic!("got both test and bench"),
        }
    }
}

fn parse_args() -> Args {
    use clap::{App, Arg, ArgGroup};
    let version = option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
    let about = r#"Compiles and runs a Rust script."#;

    let app = App::new(consts::PROGRAM_NAME)
        .version(version)
        .setting(clap::AppSettings::TrailingVarArg)
        .setting(clap::AppSettings::AllowLeadingHyphen)
        .about(about)
            .arg(Arg::new("script")
                .index(1)
                .about("Script file or expression to execute.")
                .required_unless_present_any(if cfg!(windows) {
                    vec!["clear-cache", "list-templates", "install-file-association", "uninstall-file-association"]
                } else {
                    vec!["clear-cache", "list-templates"]
                })
                .number_of_values(1)
                .conflicts_with_all(if cfg!(windows) {
                    &["list-templates", "install-file-association", "uninstall-file-association"]
                } else {
                    &["list-templates"]
                })
            )
            .arg(Arg::new("script-args")
                .index(2)
                .about("Arguments for the script to execute.")
                .multiple(true)
                .min_values(0)
            )
            .arg(Arg::new("expr")
                .about("Execute <script> as a literal expression and display the result.")
                .long("expr")
                .short('e')
                .takes_value(false)
                .requires("script")
            )
            .arg(Arg::new("loop")
                .about("Execute <script> as a literal closure once for each line from stdin.")
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
                .about("Show output from cargo when building.")
                .short('o')
                .long("cargo-output")
                .requires("script")
            )
            .arg(Arg::new("count")
                .about("Invoke the loop closure with two arguments: line, and line number.")
                .long("count")
                .requires("loop")
            )
            .arg(Arg::new("debug")
                .about("Build a debug executable, not an optimised one.")
                .long("debug")
            )
            .arg(Arg::new("dep")
                .about("Add an additional Cargo dependency. Each SPEC can be either just the package name (which will assume the latest version) or a full `name=version` spec.")
                .long("dep")
                .short('d')
                .takes_value(true)
                .multiple(true)
                .number_of_values(1)
            )
            .arg(Arg::new("dep_extern")
                .about("Like `dep`, except that it *also* adds a `#[macro_use] extern crate name;` item for expression and loop scripts. Note that this only works if the name of the dependency and the name of the library it generates are exactly the same.")
                .long("dep-extern")
                .short('D')
                .takes_value(true)
                .multiple(true)
                .requires("expr_or_loop")
            )
            .arg(Arg::new("extern")
                .about("Adds an `#[macro_use] extern crate name;` item for expressions and loop scripts.")
                .long("extern")
                .short('x')
                .takes_value(true)
                .multiple(true)
                .requires("expr_or_loop")
            )
            .arg(Arg::new("features")
                 .about("Cargo features to pass when building and running.")
                 .long("features")
                 .takes_value(true)
            )
            .arg(Arg::new("unstable_features")
                .about("Add a #![feature] declaration to the crate.")
                .long("unstable-feature")
                .short('u')
                .takes_value(true)
                .multiple(true)
                .requires("expr_or_loop")
            )

            /*
            Options that change how cargo script itself behaves, and don't alter what the script will do.
            */
            .arg(Arg::new("build_only")
                .about("Build the script, but don't run it.")
                .long("build-only")
                .requires("script")
                .conflicts_with_all(&["script-args"])
            )
            .arg(Arg::new("clear-cache")
                .about("Clears out the script cache.")
                .long("clear-cache")
            )
            .arg(Arg::new("force")
                .about("Force the script to be rebuilt.")
                .long("force")
                .requires("script")
            )
            .arg(Arg::new("gen_pkg_only")
                .about("Generate the Cargo package, but don't compile or run it.")
                .long("gen-pkg-only")
                .requires("script")
                .conflicts_with_all(&["script-args", "build_only", "debug", "force", "test", "bench"])
            )
            .arg(Arg::new("pkg_path")
                .about("Specify where to place the generated Cargo package.")
                .long("pkg-path")
                .takes_value(true)
                .requires("script")
                .conflicts_with_all(&["clear-cache", "force"])
            )
            .arg(Arg::new("test")
                .about("Compile and run tests.")
                .long("test")
                .conflicts_with_all(&["bench", "debug", "script-args", "force"])
            )
            .arg(Arg::new("bench")
                .about("Compile and run benchmarks. Requires a nightly toolchain.")
                .long("bench")
                .conflicts_with_all(&["test", "debug", "script-args", "force"])
            )
            .arg(Arg::new("template")
                .about("Specify a template to use for expression scripts.")
                .long("template")
                .short('t')
                .takes_value(true)
                .requires("expr")
            )
        .arg(Arg::new("list-templates")
            .about("List the available templates.")
            .long("list-templates")
            .takes_value(false)
        );

    #[cfg(windows)]
    let app = app
        .arg(
            Arg::new("install-file-association")
                .about("Install a file association so that rust-script executes .ers files.")
                .long("install-file-association"),
        )
        .arg(
            Arg::new("uninstall-file-association")
                .about("Uninstall the file association that makes rust-script execute .ers files.")
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

    Args {
        script: m.value_of("script").map(Into::into),
        script_args: owned_vec_string(m.values_of("script-args")),
        features: m.value_of("features").map(Into::into),

        expr: m.is_present("expr"),
        loop_: m.is_present("loop"),
        count: m.is_present("count"),

        pkg_path: m.value_of("pkg_path").map(Into::into),
        gen_pkg_only: m.is_present("gen_pkg_only"),
        build_only: m.is_present("build_only"),
        cargo_output: m.is_present("cargo-output"),
        clear_cache: m.is_present("clear-cache"),
        debug: m.is_present("debug"),
        dep: owned_vec_string(m.values_of("dep")),
        extern_: owned_vec_string(m.values_of("extern")),
        force: m.is_present("force"),
        unstable_features: owned_vec_string(m.values_of("unstable_features")),
        build_kind: BuildKind::from_flags(m.is_present("test"), m.is_present("bench")),
        template: m.value_of("template").map(Into::into),
        list_templates: m.is_present("list-templates"),
        #[cfg(windows)]
        install_file_association: m.is_present("install-file-association"),
        #[cfg(windows)]
        uninstall_file_association: m.is_present("uninstall-file-association"),
    }
}

fn main() {
    env_logger::init();

    let stderr = &mut std::io::stderr();
    match try_main() {
        Ok(0) => (),
        Ok(code) => {
            std::process::exit(code);
        }
        Err(ref err) => {
            writeln!(stderr, "error: {}", err).unwrap();
            std::process::exit(1);
        }
    }
}

fn try_main() -> MainResult<i32> {
    let args = parse_args();
    info!("Arguments: {:?}", args);

    #[cfg(windows)]
    {
        if args.install_file_association {
            file_assoc::install_file_association()?;
            return Ok(0);
        } else if args.uninstall_file_association {
            file_assoc::uninstall_file_association()?;
            return Ok(0);
        }
    }

    if args.clear_cache {
        clean_cache(0)?;
        if args.script.is_none() {
            println!("cargo script cache cleared.");
            return Ok(0);
        }
    }

    if args.list_templates {
        templates::list()?;
        return Ok(0);
    }

    // Take the arguments and work out what our input is going to be.  Primarily, this gives us the content, a user-friendly name, and a cache-friendly ID.
    // These three are just storage for the borrows we'll actually use.
    let script_name: String;
    let script_path: PathBuf;
    let content: String;

    let input = match (args.script.clone().unwrap(), args.expr, args.loop_) {
        (script, false, false) => {
            let (path, mut file) =
                find_script(&script).ok_or(format!("could not find script: {}", script))?;

            script_name = path
                .file_stem()
                .map(|os| os.to_string_lossy().into_owned())
                .unwrap_or_else(|| "unknown".into());

            let mut body = String::new();
            file.read_to_string(&mut body)?;

            let mtime = platform::file_last_modified(&file);

            script_path = std::env::current_dir()?.join(path);
            content = body;

            Input::File(&script_name, &script_path, &content, mtime)
        }
        (expr, true, false) => {
            content = expr;
            Input::Expr(&content, args.template.as_deref())
        }
        (loop_, false, true) => {
            content = loop_;
            Input::Loop(&content, args.count)
        }
        (_, _, _) => {
            panic!("Internal error: Invalid args");
        }
    };
    info!("input: {:?}", input);

    // Setup environment variables early so it's available at compilation time of scripts,
    // to allow e.g. include!(concat!(env!("RUST_SCRIPT_BASE_PATH"), "/script-module.rs"));
    std::env::set_var(
        "RUST_SCRIPT_PATH",
        input.path().unwrap_or_else(|| Path::new("")),
    );
    std::env::set_var("RUST_SCRIPT_SAFE_NAME", input.safe_name());
    std::env::set_var("RUST_SCRIPT_PKG_NAME", input.package_name());
    std::env::set_var("RUST_SCRIPT_BASE_PATH", input.base_path());

    // Sort out the dependencies.  We want to do a few things:
    // - Sort them so that they hash consistently.
    // - Check for duplicates.
    // - Expand `pkg` into `pkg=*`.
    let dependencies_from_args = {
        use std::collections::HashMap;

        let mut deps: HashMap<String, String> = HashMap::new();
        for dep in args.dep.iter().cloned() {
            // Append '=*' if it needs it.
            let dep = match dep.find('=') {
                Some(_) => dep,
                None => dep + "=*",
            };

            let mut parts = dep.splitn(2, '=');
            let name = parts.next().expect("dependency is missing name");
            let version = parts.next().expect("dependency is missing version");
            assert!(
                parts.next().is_none(),
                "dependency somehow has three parts?!"
            );

            if name == "" {
                return Err(("cannot have empty dependency package name").into());
            } else if version == "" {
                return Err(("cannot have empty dependency version").into());
            }

            if deps.insert(name.into(), version.into()).is_some() {
                return Err((format!("duplicated dependency: '{}'", name)).into());
            }
        }

        // Sort and turn into a regular vec.
        let mut deps: Vec<(String, String)> = deps.into_iter().collect();
        deps.sort();
        deps
    };

    // Generate the prelude items, if we need any. Ensure consistent and *valid* sorting.
    let prelude_items = {
        let unstable_features = args
            .unstable_features
            .iter()
            .map(|uf| format!("#![feature({})]", uf));

        let externs = args
            .extern_
            .iter()
            .map(|n| format!("#[macro_use] extern crate {};", n));

        let mut items: Vec<_> = unstable_features.chain(externs).collect();
        items.sort();
        items
    };
    info!("prelude_items: {:?}", prelude_items);

    let action = decide_action_for(&input, dependencies_from_args, prelude_items, &args)?;
    info!("action: {:?}", action);

    gen_pkg_and_compile(&input, &action)?;

    // Once we're done, clean out old packages from the cache.  There's no point if we've already done a full clear, though.
    let _defer_clear = {
        // To get around partially moved args problems.
        let cc = args.clear_cache;
        Defer::<_, MainError>::defer(move || {
            if !cc {
                clean_cache(consts::MAX_CACHE_AGE_MS)?;
            }
            Ok(())
        })
    };

    let exit_code = if action.execute {
        if action.build_kind.can_exec_directly() {
            let exe_path = get_exe_path(action.build_kind, &action.pkg_path)?;
            info!("executing {:?}", exe_path);
            let mut command = Command::new(exe_path);
            command.args(args.script_args);
            command.status().map(|st| st.code().unwrap_or(1))?
        } else {
            let cmd_name = action.build_kind.exec_command();
            info!("running `cargo {}`", cmd_name);
            let mut cmd = action.cargo(cmd_name)?;
            cmd.status().map(|st| st.code().unwrap_or(1))?
        }
    } else {
        0
    };

    Ok(exit_code)
}

/**
Clean up the cache folder.

Looks for all folders whose metadata says they were created at least `max_age` in the past and kills them dead.
*/
fn clean_cache(max_age: u128) -> MainResult<()> {
    info!("cleaning cache with max_age: {:?}", max_age);

    if max_age == 0 {
        info!("max_age is 0, clearing binary cache...");
        let cache_dir = platform::binary_cache_path()?;
        if ALLOW_AUTO_REMOVE {
            if let Err(err) = fs::remove_dir_all(&cache_dir) {
                error!("failed to remove binary cache {:?}: {}", cache_dir, err);
            }
        }
    }

    let cutoff = platform::current_time() - max_age;
    info!("cutoff:     {:>20?} ms", cutoff);

    let cache_dir = platform::generated_projects_cache_path()?;
    for child in fs::read_dir(cache_dir)? {
        let child = child?;
        let path = child.path();
        if path.is_file() {
            continue;
        }

        info!("checking: {:?}", path);

        let remove_dir = || {
            /*
            Ok, so *why* aren't we using `modified in the package metadata?  The point of *that* is to track what we know about the input.  The problem here is that `--expr` and `--loop` don't *have* modification times; they just *are*.

            Now, `PackageMetadata` *could* be modified to store, say, the moment in time the input was compiled, but then we couldn't use that field for metadata matching when decided whether or not a *file* input should be recompiled.

            So, instead, we're just going to go by the timestamp on the metadata file *itself*.
            */
            let meta_mtime = {
                let meta_path = get_pkg_metadata_path(&path);
                let meta_file = match fs::File::open(&meta_path) {
                    Ok(file) => file,
                    Err(..) => {
                        info!("couldn't open metadata for {:?}", path);
                        return true;
                    }
                };
                platform::file_last_modified(&meta_file)
            };
            info!("meta_mtime: {:>20?} ms", meta_mtime);

            meta_mtime <= cutoff
        };

        if remove_dir() {
            info!("removing {:?}", path);
            if ALLOW_AUTO_REMOVE {
                if let Err(err) = fs::remove_dir_all(&path) {
                    error!("failed to remove {:?} from cache: {}", path, err);
                }
            } else {
                info!("(suppressed remove)");
            }
        }
    }
    info!("done cleaning cache.");
    Ok(())
}

/**
Generate and compile a package from the input.

Why take `PackageMetadata`?  To ensure that any information we need to depend on for compilation *first* passes through `decide_action_for` *and* is less likely to not be serialised with the rest of the metadata.
*/
fn gen_pkg_and_compile(input: &Input, action: &InputAction) -> MainResult<()> {
    let pkg_path = &action.pkg_path;
    let meta = &action.metadata;
    let old_meta = action.old_metadata.as_ref();

    let mani_str = &action.manifest;
    let script_str = &action.script;

    info!("creating pkg dir...");
    fs::create_dir_all(pkg_path)?;
    let cleanup_dir: Defer<_, MainError> = Defer::defer(|| {
        // DO NOT try deleting ANYTHING if we're not cleaning up inside our own cache.  We *DO NOT* want to risk killing user files.
        if action.using_cache {
            info!("cleaning up cache directory {:?}", pkg_path);
            if ALLOW_AUTO_REMOVE {
                fs::remove_dir_all(pkg_path)?;
            } else {
                info!("(suppressed remove)");
            }
        }
        Ok(())
    });

    let mut meta = meta.clone();

    info!("generating Cargo package...");
    let mani_path = {
        let mani_path = action.manifest_path();
        let mani_hash = old_meta.map(|m| &*m.manifest_hash);
        match overwrite_file(&mani_path, mani_str, mani_hash)? {
            FileOverwrite::Same => (),
            FileOverwrite::Changed { new_hash } => {
                meta.manifest_hash = new_hash;
            }
        }
        mani_path
    };

    {
        let script_path = pkg_path.join(format!("{}.rs", input.safe_name()));
        /*
        There are times (particularly involving shared target dirs) where we can't rely on Cargo to correctly detect invalidated builds.  As such, if we've been told to *force* a recompile, we'll deliberately force the script to be overwritten, which will invalidate the timestamp, which will lead to a recompile.
        */
        let script_hash = if action.force_compile {
            debug!("told to force compile, ignoring script hash");
            None
        } else {
            old_meta.map(|m| &*m.script_hash)
        };
        match overwrite_file(&script_path, script_str, script_hash)? {
            FileOverwrite::Same => (),
            FileOverwrite::Changed { new_hash } => {
                meta.script_hash = new_hash;
            }
        }
    }

    let meta = meta;

    /*
    *bursts through wall* It's Cargo Time! (Possibly)

    Note that there's a complication here: we want to *temporarily* continue *even if compilation fails*.  This is because if we don't, then every time you run `cargo script` on a script you're currently modifying, and it fails to compile, your compiled dependencies get obliterated.

    This is *really* annoying.

    As such, we want to ignore any compilation problems until *after* we've written the metadata and disarmed the cleanup callback.
    */
    if action.compile {
        info!("compiling...");
        let mut cmd = cargo(
            "build",
            &*mani_path.to_string_lossy(),
            action.use_nightly,
            &meta,
        )?;

        let exit_status = if action.cargo_output {
            cmd.spawn()?.wait()
        } else {
            util::suppress_child_output(&mut cmd)?.status()
        };

        let compile_err =
            exit_status
                .map_err(Into::<MainError>::into)
                .and_then(|st| match st.code() {
                    Some(0) => Ok(()),
                    Some(st) => Err(format!("cargo failed with status {}", st).into()),
                    None => Err("cargo failed".into()),
                });

        // Drop out now if compilation failed.
        let _ = compile_err?;

        // Find out and cache what the executable was called.
        let _ = cargo_target(
            input,
            pkg_path,
            &*mani_path.to_string_lossy(),
            action.use_nightly,
            &meta,
        )?;

        // Write out the metadata hash to tie this executable to a particular chunk of metadata.  This is to avoid issues with multiple scripts with the same name being compiled to a common target directory.
        let meta_hash = action.metadata.sha1_hash();
        info!("writing meta hash: {:?}...", meta_hash);
        let exe_meta_hash_path = get_meta_hash_path(pkg_path)?;
        let mut f = fs::File::create(&exe_meta_hash_path)?;
        write!(&mut f, "{}", meta_hash)?;
    }

    // Write out metadata *now*.  Remember that we check the timestamp in the metadata, *not* on the executable.
    if action.emit_metadata {
        info!("emitting metadata...");
        write_pkg_metadata(pkg_path, &meta)?;
    }

    info!("disarming pkg dir cleanup...");
    cleanup_dir.disarm();

    Ok(())
}

/**
This represents what to do with the input provided by the user.
*/
#[derive(Debug)]
struct InputAction {
    /// Compile the input into a fresh executable?
    compile: bool,

    /// Always show cargo output?
    cargo_output: bool,

    /**
    Force Cargo to do a recompile, even if it thinks it doesn't have to.

    `compile` must be `true` for this to have any effect.
    */
    force_compile: bool,

    /// Emit a metadata file?
    emit_metadata: bool,

    /// Execute the compiled binary?
    execute: bool,

    /// Directory where the package should live.
    pkg_path: PathBuf,

    /**
    Is the package directory in the cache?

    Currently, this can be inferred from `emit_metadata`, but there's no *intrinsic* reason they should be tied together.
    */
    using_cache: bool,

    use_nightly: bool,

    /// The package metadata structure for the current invocation.
    metadata: PackageMetadata,

    /// The package metadata structure for the *previous* invocation, if it exists.
    old_metadata: Option<PackageMetadata>,

    /// The package manifest contents.
    manifest: String,

    /// The script source.
    script: String,

    /// Did the user ask to run tests or benchmarks?
    build_kind: BuildKind,
}

impl InputAction {
    fn manifest_path(&self) -> PathBuf {
        self.pkg_path.join("Cargo.toml")
    }

    fn cargo(&self, cmd: &str) -> MainResult<Command> {
        cargo(
            cmd,
            &*self.manifest_path().to_string_lossy(),
            self.use_nightly,
            &self.metadata,
        )
    }
}

/**
The metadata here serves two purposes:

1. It records everything necessary for compilation and execution of a package.
2. It records everything that must be exactly the same in order for a cached executable to still be valid, in addition to the content hash.
*/
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct PackageMetadata {
    /// Path to the script file.
    path: Option<String>,

    /// Last-modified timestamp for script file.
    modified: Option<u128>,

    /// Template used.
    template: Option<String>,

    /// Was the script compiled in debug mode?
    debug: bool,

    /// Sorted list of dependencies.
    deps: Vec<(String, String)>,

    /// Sorted list of injected prelude items.
    prelude: Vec<String>,

    /// Cargo features
    features: Option<String>,

    /// Hash of the generated `Cargo.toml` file.
    manifest_hash: String,

    /// Hash of the generated source file.
    script_hash: String,
}

impl PackageMetadata {
    pub fn sha1_hash(&self) -> String {
        // Yes, I *do* feel dirty for doing it like this.  :D
        hash_str(&format!("{:?}", self))
    }
}

/**
For the given input, this constructs the package metadata and checks the cache to see what should be done.
*/
fn decide_action_for(
    input: &Input,
    deps: Vec<(String, String)>,
    prelude: Vec<String>,
    args: &Args,
) -> MainResult<InputAction> {
    let (pkg_path, using_cache) = args
        .pkg_path
        .as_ref()
        .map(|p| (p.into(), false))
        .unwrap_or_else(|| {
            // This can't fail.  Seriously, we're *fucked* if we can't work this out.
            let cache_path = platform::generated_projects_cache_path().unwrap();

            let id = {
                let deps_iter = deps.iter().map(|&(ref n, ref v)| (n as &str, v as &str));

                // Again, also fucked if we can't work this out.
                input.compute_id(deps_iter).unwrap()
            };
            info!("id: {:?}", id);

            (cache_path.join(&id), true)
        });
    info!("pkg_path: {:?}", pkg_path);
    info!("using_cache: {:?}", using_cache);

    let (mani_str, script_str) = manifest::split_input(input, &deps, &prelude)?;

    // Forcibly override some flags based on build kind.
    let (debug, force, build_only) = match args.build_kind {
        BuildKind::Normal => (args.debug, args.force, args.build_only),
        BuildKind::Test => (true, false, false),
        BuildKind::Bench => (false, false, false),
    };

    let input_meta = {
        let (path, mtime, template) = match *input {
            Input::File(_, path, _, mtime) => {
                (Some(path.to_string_lossy().into_owned()), Some(mtime), None)
            }
            Input::Expr(_, template) => (None, None, template),
            Input::Loop(..) => (None, None, None),
        };
        PackageMetadata {
            path,
            modified: mtime,
            template: template.map(Into::into),
            debug,
            deps,
            prelude,
            features: args.features.clone(),
            manifest_hash: hash_str(&mani_str),
            script_hash: hash_str(&script_str),
        }
    };
    info!("input_meta: {:?}", input_meta);

    let mut action = InputAction {
        compile: force,
        cargo_output: args.cargo_output,
        force_compile: force,
        emit_metadata: true,
        execute: !build_only,
        pkg_path,
        using_cache,
        use_nightly: matches!(args.build_kind, BuildKind::Bench),
        metadata: input_meta,
        old_metadata: None,
        manifest: mani_str,
        script: script_str,
        build_kind: args.build_kind,
    };

    macro_rules! bail {
        ($($name:ident: $value:expr),*) => {
            return Ok(InputAction {
                $($name: $value,)*
                ..action
            })
        }
    }

    // If we were told to only generate the package, we need to stop *now*
    if args.gen_pkg_only {
        bail!(compile: false, execute: false)
    }

    // If we're not doing a regular build, stop.
    match action.build_kind {
        BuildKind::Normal => (),
        BuildKind::Test | BuildKind::Bench => {
            info!("not recompiling because: user asked for test/bench");
            bail!(compile: false, force_compile: false)
        }
    }

    let cache_meta = match get_pkg_metadata(&action.pkg_path) {
        Ok(meta) => meta,
        Err(err) => {
            info!(
                "recompiling since failed to load metadata: {}",
                err.to_string()
            );
            bail!(compile: true)
        }
    };

    if cache_meta != action.metadata {
        info!("recompiling because: metadata did not match");
        debug!("input metadata: {:?}", action.metadata);
        debug!("cache metadata: {:?}", cache_meta);
        bail!(old_metadata: Some(cache_meta), compile: true)
    }

    action.old_metadata = Some(cache_meta);

    let exe_exists = match get_exe_path(action.build_kind, &action.pkg_path) {
        Ok(exe_path) => exe_path.is_file(),
        Err(_) => false,
    };
    if !exe_exists {
        info!("recompiling because: executable doesn't exist or isn't a file");
        bail!(compile: true)
    }

    // Finally  check to see if `{exe_path}.meta-hash` exists and contains a hash that matches the metadata.
    // We need to do this to account for cases where Cargo's target directory has been set to a fixed, shared location.
    let exe_meta_hash_path = get_meta_hash_path(&action.pkg_path).unwrap();
    if !exe_meta_hash_path.is_file() {
        info!("recompiling because: meta hash doesn't exist or isn't a file");
        bail!(compile: true, force_compile: true)
    }
    let exe_meta_hash = {
        let mut f = fs::File::open(&exe_meta_hash_path)?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        s
    };
    let meta_hash = action.metadata.sha1_hash();
    if meta_hash != exe_meta_hash {
        info!("recompiling because: meta hash doesn't match");
        bail!(compile: true, force_compile: true)
    }

    Ok(action)
}

/**
Figures out where the output executable for the input should be.

This *requires* that `cargo_target` has already been called on the package.
*/
fn get_exe_path<P>(build_kind: BuildKind, pkg_path: P) -> MainResult<PathBuf>
where
    P: AsRef<Path>,
{
    use std::fs::File;

    // We don't directly run tests and benchmarks.
    match build_kind {
        BuildKind::Normal => (),
        BuildKind::Test | BuildKind::Bench => {
            return Err("tried to get executable path for test/bench build".into());
        }
    }

    let package_path = pkg_path.as_ref();
    let cache_path = package_path.join("target.exe_path");

    let mut f = File::open(&cache_path)?;
    let exe_path = platform::read_path(&mut f)?;

    Ok(exe_path)
}

/**
Figures out where the `meta-hash` file should be.
*/
fn get_meta_hash_path<P>(pkg_path: P) -> MainResult<PathBuf>
where
    P: AsRef<Path>,
{
    Ok(pkg_path.as_ref().join("target.meta-hash"))
}

/**
Load the package metadata, given the path to the package's cache folder.
*/
fn get_pkg_metadata<P>(pkg_path: P) -> MainResult<PackageMetadata>
where
    P: AsRef<Path>,
{
    let meta_path = get_pkg_metadata_path(pkg_path);
    debug!("meta_path: {:?}", meta_path);
    let mut meta_file = fs::File::open(&meta_path)?;

    let meta_str = {
        let mut s = String::new();
        meta_file.read_to_string(&mut s).unwrap();
        s
    };
    let meta: PackageMetadata = serde_json::from_str(&meta_str).map_err(|err| err.to_string())?;

    Ok(meta)
}

/**
Work out the path to a package's metadata file.
*/
fn get_pkg_metadata_path<P>(pkg_path: P) -> PathBuf
where
    P: AsRef<Path>,
{
    pkg_path.as_ref().join(consts::METADATA_FILE)
}

/**
Save the package metadata, given the path to the package's cache folder.
*/
fn write_pkg_metadata<P>(pkg_path: P, meta: &PackageMetadata) -> MainResult<()>
where
    P: AsRef<Path>,
{
    let meta_path = get_pkg_metadata_path(pkg_path);
    debug!("meta_path: {:?}", meta_path);
    let mut meta_file = fs::File::create(&meta_path)?;
    let meta_str = serde_json::to_string(meta).map_err(|err| err.to_string())?;
    write!(&mut meta_file, "{}", meta_str)?;
    meta_file.flush()?;
    Ok(())
}

/**
Attempts to locate the script specified by the given path.  If the path as-given doesn't yield anything, it will try adding file extensions.
*/
fn find_script<P>(path: P) -> Option<(PathBuf, fs::File)>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();

    // Try the path directly.
    if let Ok(file) = fs::File::open(path) {
        return Some((path.into(), file));
    }

    // If it had an extension, don't bother trying any others.
    if path.extension().is_some() {
        return None;
    }

    // Ok, now try other extensions.
    for &ext in consts::SEARCH_EXTS {
        let path = path.with_extension(ext);
        if let Ok(file) = fs::File::open(&path) {
            return Some((path, file));
        }
    }

    // Welp. ¯\_(ツ)_/¯
    None
}

/**
Represents an input source for a script.
*/
#[derive(Clone, Debug)]
pub enum Input<'a> {
    /**
    The input is a script file.

    The tuple members are: the name, absolute path, script contents, last modified time.
    */
    File(&'a str, &'a Path, &'a str, u128),

    /**
    The input is an expression.

    The tuple member is: the script contents, and the template (if any).
    */
    Expr(&'a str, Option<&'a str>),

    /**
    The input is a loop expression.

    The tuple member is: the script contents, whether the `--count` flag was given.
    */
    Loop(&'a str, bool),
}

impl<'a> Input<'a> {
    /**
    Return the path to the script, if it has one.
    */
    pub const fn path(&self) -> Option<&Path> {
        use crate::Input::*;

        match *self {
            File(_, path, _, _) => Some(path),
            Expr(..) => None,
            Loop(..) => None,
        }
    }

    /**
    Return the "safe name" for the input.  This should be filename-safe.

    Currently, nothing is done to ensure this, other than hoping *really hard* that we don't get fed some excessively bizzare input filename.
    */
    pub const fn safe_name(&self) -> &str {
        use crate::Input::*;

        match *self {
            File(name, _, _, _) => name,
            Expr(..) => "expr",
            Loop(..) => "loop",
        }
    }

    /**
    Return the package name for the input.  This should be a valid Rust identifier.
    */
    pub fn package_name(&self) -> String {
        let name = self.safe_name();
        let mut r = String::with_capacity(name.len());

        for (i, c) in name.chars().enumerate() {
            match (i, c) {
                (0, '0'..='9') => {
                    r.push('_');
                    r.push(c);
                }
                (_, '0'..='9') | (_, 'a'..='z') | (_, 'A'..='Z') | (_, '_') | (_, '-') => {
                    r.push(c);
                }
                (_, _) => {
                    r.push('_');
                }
            }
        }

        r
    }

    /**
    Base directory for resolving relative paths.
    */
    pub fn base_path(&self) -> PathBuf {
        match *self {
            Input::File(_, path, _, _) => path
                .parent()
                .expect("couldn't get parent directory for file input base path")
                .into(),
            Input::Expr(..) | Input::Loop(..) => {
                std::env::current_dir().expect("couldn't get current directory for input base path")
            }
        }
    }

    /**
    Compute the package ID for the input.  This is used as the name of the cache folder into which the Cargo package will be generated.
    */
    pub fn compute_id<'dep, DepIt>(&self, deps: DepIt) -> MainResult<OsString>
    where
        DepIt: IntoIterator<Item = (&'dep str, &'dep str)>,
    {
        use crate::Input::*;

        let hash_deps = || {
            let mut hasher = Sha1::new();
            for dep in deps {
                hasher.update(b"dep=");
                hasher.update(dep.0);
                hasher.update(b"=");
                hasher.update(dep.1);
                hasher.update(b";");
            }
            hasher
        };

        match *self {
            File(name, path, _, _) => {
                let mut hasher = Sha1::new();

                // Hash the path to the script.
                hasher.update(&*path.to_string_lossy());
                let mut digest = format!("{:x}", hasher.finalize());
                digest.truncate(consts::ID_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push("file-");
                id.push(name);
                id.push("-");
                id.push(if STUB_HASHES { "stub" } else { &*digest });
                Ok(id)
            }
            Expr(content, template) => {
                let mut hasher = hash_deps();

                hasher.update("template:");
                hasher.update(template.unwrap_or(""));
                hasher.update(";");

                hasher.update(content);
                let mut digest = format!("{:x}", hasher.finalize());
                digest.truncate(consts::ID_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push("expr-");
                id.push(if STUB_HASHES { "stub" } else { &*digest });
                Ok(id)
            }
            Loop(content, count) => {
                let mut hasher = hash_deps();

                // Make sure to include the [non-]presence of the `--count` flag in the flag, since it changes the actual generated script output.
                hasher.update("count:");
                hasher.update(if count { "true;" } else { "false;" });

                hasher.update(content);
                let mut digest = format!("{:x}", hasher.finalize());
                digest.truncate(consts::ID_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push("loop-");
                id.push(if STUB_HASHES { "stub" } else { &*digest });
                Ok(id)
            }
        }
    }
}

/**
Shorthand for hashing a string.
*/
fn hash_str(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s);
    format!("{:x}", hasher.finalize())
}

enum FileOverwrite {
    Same,
    Changed { new_hash: String },
}

/**
Overwrite a file if and only if the contents have changed.
*/
fn overwrite_file<P>(path: P, content: &str, hash: Option<&str>) -> MainResult<FileOverwrite>
where
    P: AsRef<Path>,
{
    debug!("overwrite_file({:?}, _, {:?})", path.as_ref(), hash);
    let new_hash = hash_str(content);
    if Some(&*new_hash) == hash {
        debug!(".. hashes match");
        return Ok(FileOverwrite::Same);
    }

    debug!(".. hashes differ; new_hash: {:?}", new_hash);
    let mut file = fs::File::create(path)?;
    write!(&mut file, "{}", content)?;
    file.flush()?;
    Ok(FileOverwrite::Changed { new_hash })
}

/**
Constructs a Cargo command that runs on the script package.
*/
fn cargo(
    cmd_name: &str,
    manifest: &str,
    nightly: bool,
    meta: &PackageMetadata,
) -> MainResult<Command> {
    let mut cmd = Command::new("cargo");
    if nightly {
        cmd.arg("+nightly");
    }
    cmd.arg(cmd_name).arg("--manifest-path").arg(manifest);

    if platform::force_cargo_color() {
        cmd.arg("--color").arg("always");
    }

    let script_specific_binary_cache = format!(
        "{}/{}",
        platform::binary_cache_path()?.display(),
        meta.sha1_hash()
    );
    cmd.env("CARGO_TARGET_DIR", script_specific_binary_cache);

    // Block `--release` on `bench`.
    if !meta.debug && cmd_name != "bench" {
        cmd.arg("--release");
    }

    if let Some(ref features) = meta.features {
        cmd.arg("--features").arg(features);
    }

    Ok(cmd)
}

/**
Tries to find the path to a package's target file.

This will also cache this information such that `exe_path` can find it later.
*/
fn cargo_target<P>(
    input: &Input,
    pkg_path: P,
    manifest: &str,
    use_nightly: bool,
    meta: &PackageMetadata,
) -> MainResult<PathBuf>
where
    P: AsRef<Path>,
{
    trace!(
        "cargo_target(_, {:?}, {:?}, _)",
        pkg_path.as_ref(),
        manifest,
    );

    let exe_path = cargo_target_by_message(input, manifest, use_nightly, meta)?;

    trace!(".. exe_path: {:?}", exe_path);

    // Before we return, cache the result.
    {
        use std::fs::File;

        let manifest_path = Path::new(manifest);
        let package_path = manifest_path.parent().unwrap();
        let cache_path = package_path.join("target.exe_path");

        let mut f = File::create(&cache_path)?;
        platform::write_path(&mut f, &exe_path)?;
    }

    Ok(exe_path)
}

// Gets the path to the package's target file by parsing the output of `cargo build`.
fn cargo_target_by_message(
    input: &Input,
    manifest: &str,
    use_nightly: bool,
    meta: &PackageMetadata,
) -> MainResult<PathBuf> {
    use std::io::{BufRead, BufReader};

    let mut cmd = cargo("build", manifest, use_nightly, meta)?;
    cmd.arg("--message-format=json");
    cmd.stdout(process::Stdio::piped());
    cmd.stderr(process::Stdio::null());

    trace!(".. cmd: {:?}", cmd);

    let mut child = cmd.spawn()?;

    let package_name = input.package_name();

    let stdout = BufReader::new(child.stdout.take().unwrap());
    let mut lines = stdout.lines();

    #[derive(Deserialize)]
    struct Target {
        name: String,
    }

    #[derive(Deserialize)]
    struct Line {
        reason: String,
        target: Target,
        filenames: Vec<PathBuf>,
    }

    while let Some(Ok(line)) = lines.next() {
        if let Ok(mut l) = serde_json::from_str::<Line>(&line).map_err(Box::new) {
            if l.reason == "compiler-artifact" && l.target.name == package_name {
                let _ = child.kill();
                return Ok(l.filenames.swap_remove(0));
            }
        }
    }

    match child.wait()?.code() {
        Some(st) => Err(format!(
            "could not determine target filename: cargo exited with status {}",
            st
        )
        .into()),
        None => Err(
            "could not determine target filename: cargo exited abnormally"
                .to_string()
                .into(),
        ),
    }
}
