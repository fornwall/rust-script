/*
Copyright ⓒ 2015-2017 cargo-script contributors.

Licensed under the MIT license (see LICENSE or <http://opensource.org
/licenses/MIT>) or the Apache License, Version 2.0 (see LICENSE of
<http://www.apache.org/licenses/LICENSE-2.0>), at your option. All
files in the project carrying such notice may not be copied, modified,
or distributed except according to those terms.
*/
/*!
`cargo-script` is a Cargo subcommand designed to let people quickly and easily run Rust "scripts" which can make use of Cargo's package ecosystem.

Or, to put it in other words, it lets you write useful, but small, Rust programs without having to create a new directory and faff about with `Cargo.toml`.

As such, `cargo-script` does two major things:

1. Given a script, it extracts the embedded Cargo manifest and merges it with some sensible defaults.  This manifest, along with the source code, is written to a fresh Cargo package on-disk.

2. It caches the generated and compiled packages, regenerating them only if the script or its metadata have changed.
*/
extern crate clap;
extern crate env_logger;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
extern crate open;
extern crate regex;
extern crate rustc_serialize;
extern crate semver;
extern crate shaman;
extern crate toml;

#[cfg(feature="chan")]
#[macro_use] extern crate chan;

/**
If this is set to `true`, the digests used for package IDs will be replaced with "stub" to make testing a bit easier.  Obviously, you don't want this `true` for release...
*/
const STUB_HASHES: bool = false;

/**
If this is set to `false`, then code that automatically deletes stuff *won't*.
*/
const ALLOW_AUTO_REMOVE: bool = true;

/**
Length of time to suppress Cargo output.
*/
#[cfg(feature="suppress-cargo-output")]
const CARGO_OUTPUT_TIMEOUT: u64 = 2_000/*ms*/;

// This macro exists for 1.11 support.
#[cfg(windows)]
macro_rules! if_windows {
    (@as_expr $e:expr) => { $e };
    ($($tts:tt)*) => { if_windows! { @as_expr { $($tts)* } } };
}

#[cfg(not(windows))]
macro_rules! if_windows {
    ($($tts:tt)*) => { {} };
}

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

use std::borrow::Cow;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use semver::Version;

use error::{Blame, MainError, Result, ResultExt};
use platform::MigrationKind;
use util::{ChainMap, Defer, PathExt};

#[derive(Debug)]
enum SubCommand {
    Script(Args),
    Templates(templates::Args),
    #[cfg(windows)]
    FileAssoc(file_assoc::Args),
}

#[derive(Debug)]
struct Args {
    script: Option<String>,
    args: Vec<String>,
    features: Option<String>,

    expr: bool,
    loop_: bool,
    count: bool,

    pkg_path: Option<String>,
    gen_pkg_only: bool,
    build_only: bool,
    clear_cache: bool,
    debug: bool,
    dep: Vec<String>,
    dep_extern: Vec<String>,
    extern_: Vec<String>,
    force: bool,
    unstable_features: Vec<String>,
    use_bincache: Option<bool>,
    migrate_data: Option<MigrationKind>,
    build_kind: BuildKind,
    template: Option<String>,
}

#[derive(Copy, Clone, Debug)]
enum BuildKind {
    Normal,
    Test,
    Bench,
}

impl BuildKind {
    fn can_exec_directly(&self) -> bool {
        match *self {
            BuildKind::Normal => true,
            BuildKind::Test | BuildKind::Bench => false,
        }
    }

    fn exec_command(&self) -> &'static str {
        match *self {
            BuildKind::Normal => panic!("asked for exec command for normal build"),
            BuildKind::Test => "test",
            BuildKind::Bench => "bench",
        }
    }

    fn from_flags(test: bool, bench: bool) -> Self {
        match (test, bench) {
            (false, false) => BuildKind::Normal,
            (true, false) => BuildKind::Test,
            (false, true) => BuildKind::Bench,
            _ => panic!("got both test and bench")
        }
    }
}

fn parse_args() -> SubCommand {
    use clap::{App, Arg, ArgGroup, SubCommand, AppSettings};
    let version = option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
    let about = r#"Compiles and runs "Cargoified Rust scripts"."#;

    // "const str array slice"
    macro_rules! csas {
        ($($es:expr),*) => {
            {
                const PIN: &'static [&'static str] = &[$($es),*];
                PIN
            }
        }
    }

    // We have to kinda lie about who we are for the output to look right...
    let m = App::new("cargo")
        .bin_name("cargo")
        .version(version)
        .about(about)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(SubCommand::with_name("script")
            .version(version)
            .about(about)
            .usage("cargo script [FLAGS OPTIONS] [--] <script> <args>...")

            /*
            Major script modes.
            */
            .arg(Arg::with_name("script")
                .help("Script file (with or without extension) to execute.")
                .index(1)
            )
            .arg(Arg::with_name("args")
                .help("Additional arguments passed to the script.")
                .index(2)
                .multiple(true)
            )
            .arg(Arg::with_name("expr")
                .help("Execute <script> as a literal expression and display the result.")
                .long("expr")
                .short("e")
                .conflicts_with_all(csas!["loop"])
                .requires("script")
            )
            .arg(Arg::with_name("loop")
                .help("Execute <script> as a literal closure once for each line from stdin.")
                .long("loop")
                .short("l")
                .conflicts_with_all(csas!["expr"])
                .requires("script")
            )
            .group(ArgGroup::with_name("expr_or_loop")
                .args(&["expr", "loop"])
            )

            /*
            Options that impact the script being executed.
            */
            .arg(Arg::with_name("count")
                .help("Invoke the loop closure with two arguments: line, and line number.")
                .long("count")
                .requires("loop")
            )
            .arg(Arg::with_name("debug")
                .help("Build a debug executable, not an optimised one.")
                .long("debug")
                .requires("script")
            )
            .arg(Arg::with_name("dep")
                .help("Add an additional Cargo dependency.  Each SPEC can be either just the package name (which will assume the latest version) or a full `name=version` spec.")
                .long("dep")
                .short("d")
                .takes_value(true)
                .multiple(true)
                .number_of_values(1)
                .requires("script")
            )
            .arg(Arg::with_name("dep_extern")
                .help("Like `dep`, except that it *also* adds a `#[macro_use] extern crate name;` item for expression and loop scripts.  Note that this only works if the name of the dependency and the name of the library it generates are exactly the same.")
                .long("dep-extern")
                .short("D")
                .takes_value(true)
                .multiple(true)
                .requires("expr_or_loop")
            )
            .arg(Arg::with_name("extern")
                .help("Adds an `#[macro_use] extern crate name;` item for expressions and loop scripts.")
                .long("extern")
                .short("x")
                .takes_value(true)
                .multiple(true)
                .requires("expr_or_loop")
            )
            .arg(Arg::with_name("features")
                 .help("Cargo features to pass when building and running.")
                 .long("features")
                 .takes_value(true)
            )
            .arg(Arg::with_name("unstable_features")
                .help("Add a #![feature] declaration to the crate.")
                .long("unstable-feature")
                .short("u")
                .takes_value(true)
                .multiple(true)
                .requires("expr_or_loop")
            )

            /*
            Options that change how cargo script itself behaves, and don't alter what the script will do.
            */
            .arg(Arg::with_name("build_only")
                .help("Build the script, but don't run it.")
                .long("build-only")
                .requires("script")
                .conflicts_with_all(csas!["args"])
            )
            .arg(Arg::with_name("clear_cache")
                .help("Clears out the script cache.")
                .long("clear-cache")
            )
            .arg(Arg::with_name("force")
                .help("Force the script to be rebuilt.")
                .long("force")
                .requires("script")
            )
            .arg(Arg::with_name("gen_pkg_only")
                .help("Generate the Cargo package, but don't compile or run it.")
                .long("gen-pkg-only")
                .requires("script")
                .conflicts_with_all(csas!["args", "build_only", "debug", "force", "test", "bench"])
            )
            .arg(Arg::with_name("pkg_path")
                .help("Specify where to place the generated Cargo package.")
                .long("pkg-path")
                .takes_value(true)
                .requires("script")
                .conflicts_with_all(csas!["clear_cache", "force"])
            )
            .arg(Arg::with_name("use_bincache")
                .help("Override whether or not the shared binary cache will be used for compilation.")
                .long("use-shared-binary-cache")
                .takes_value(true)
                .possible_values(csas!["no", "yes"])
            )
            .arg(Arg::with_name("migrate_data")
                .help("Migrate data from older versions.")
                .long("migrate-data")
                .takes_value(true)
                .possible_values(csas!["dry-run", "for-real"])
            )
            .arg(Arg::with_name("test")
                .help("Compile and run tests.")
                .long("test")
                .conflicts_with_all(csas!["bench", "debug", "args", "force"])
            )
            .arg(Arg::with_name("bench")
                .help("Compile and run benchmarks.  Requires a nightly toolchain.")
                .long("bench")
                .conflicts_with_all(csas!["test", "debug", "args", "force"])
            )
            .arg(Arg::with_name("template")
                .help("Specify a template to use for expression scripts.")
                .long("template")
                .short("t")
                .takes_value(true)
                .requires("expr")
            )
        )
        .subcommand(templates::Args::subcommand())
        .chain_map(|mut app| {
            drop(&mut app); // avoid warning
            if_windows! {
                app = app.subcommand(file_assoc::Args::subcommand());
            }
            app
        })
        .get_matches();

    if let Some(m) = m.subcommand_matches("templates") {
        return ::SubCommand::Templates(templates::Args::parse(m));
    }

    if_windows! {
        if let Some(m) = m.subcommand_matches("file-association") {
            return ::SubCommand::FileAssoc(file_assoc::Args::parse(m));
        }
    }

    let m = m.subcommand_matches("script").unwrap();

    fn owned_vec_string<'a, I>(v: Option<I>) -> Vec<String>
    where I: ::std::iter::Iterator<Item=&'a str> {
        v.map(|itr| itr.map(Into::into).collect()).unwrap_or(vec![])
    }

    fn yes_or_no(v: Option<&str>) -> Option<bool> {
        v.map(|v| match v {
            "yes" => true,
            "no" => false,
            _ => unreachable!()
        })
    }

    fn run_kind(v: Option<&str>) -> Option<MigrationKind> {
        v.map(|v| match v {
            "dry-run" => MigrationKind::DryRun,
            "for-real" => MigrationKind::ForReal,
            _ => unreachable!()
        })
    }

    ::SubCommand::Script(Args {
        script: m.value_of("script").map(Into::into),
        args: owned_vec_string(m.values_of("args")),
        features: m.value_of("features").map(Into::into),

        expr: m.is_present("expr"),
        loop_: m.is_present("loop"),
        count: m.is_present("count"),

        pkg_path: m.value_of("pkg_path").map(Into::into),
        gen_pkg_only: m.is_present("gen_pkg_only"),
        build_only: m.is_present("build_only"),
        clear_cache: m.is_present("clear_cache"),
        debug: m.is_present("debug"),
        dep: owned_vec_string(m.values_of("dep")),
        dep_extern: owned_vec_string(m.values_of("dep_extern")),
        extern_: owned_vec_string(m.values_of("extern")),
        force: m.is_present("force"),
        unstable_features: owned_vec_string(m.values_of("unstable_features")),
        use_bincache: yes_or_no(m.value_of("use_bincache")),
        migrate_data: run_kind(m.value_of("migrate_data")),
        build_kind: BuildKind::from_flags(m.is_present("test"), m.is_present("bench")),
        template: m.value_of("template").map(Into::into),
    })
}

fn main() {
    env_logger::init().unwrap();
    info!("starting");
    info!("args: {:?}", std::env::args().collect::<Vec<_>>());
    let stderr = &mut std::io::stderr();
    match try_main() {
        Ok(0) => (),
        Ok(code) => {
            std::process::exit(code);
        },
        Err(ref err) if err.is_human() => {
            writeln!(stderr, "error: {}", err).unwrap();
            std::process::exit(1);
        },
        Err(ref err) => {
            writeln!(stderr, "internal error: {}", err).unwrap();
            std::process::exit(1);
        }
    }
}

fn try_main() -> Result<i32> {
    let args = parse_args();
    info!("Arguments: {:?}", args);

    let args = match args {
        SubCommand::Script(args) => args,
        SubCommand::Templates(args) => return templates::try_main(args),
        #[cfg(windows)]
        SubCommand::FileAssoc(args) => return file_assoc::try_main(args),
    };

    /*
    Do data migration before anything else, since it can cause the location of stuff to change.
    */
    if let Some(run_kind) = args.migrate_data {
        println!("Migrating data...");
        let (log, res) = platform::migrate_old_data(run_kind);
        match (log.len(), res) {
            (0, Ok(())) => {
                println!("Nothing to do.");
                return Ok(0);
            },
            (_, Ok(())) => {
                for entry in log {
                    println!("- {}", entry);
                }
                return Ok(0);
            },
            (_, Err(err)) => {
                for entry in log {
                    println!("- {}", entry);
                }
                return Err(err);
            }
        }
    }

    if log_enabled!(log::LogLevel::Debug) {
        let scp = get_script_cache_path()?;
        let bcp = get_binary_cache_path()?;
        debug!("script-cache path: {:?}", scp);
        debug!("binary-cache path: {:?}", bcp);
    }

    /*
    If we've been asked to clear the cache, do that *now*.  There are two reasons:

    1. Do it *before* we call `decide_action_for` such that this flag *also* acts as a synonym for `--force`.
    2. Do it *before* we start trying to read the input so that, later on, we can make `<script>` optional, but still supply `--clear-cache`.
    */
    if args.clear_cache {
        clean_cache(0)?;

        // If we *did not* get a `<script>` argument, that's OK.
        if args.script.is_none() {
            // Just let the user know that we did *actually* run.
            println!("cargo script cache cleared.");
            return Ok(0);
        }
    }

    // Take the arguments and work out what our input is going to be.  Primarily, this gives us the content, a user-friendly name, and a cache-friendly ID.
    // These three are just storage for the borrows we'll actually use.
    let script_name: String;
    let script_path: PathBuf;
    let content: String;

    let input = match (args.script, args.expr, args.loop_) {
        (Some(script), false, false) => {
            let (path, mut file) = find_script(script).ok_or("could not find script")?;

            script_name = path.file_stem()
                .map(|os| os.to_string_lossy().into_owned())
                .unwrap_or("unknown".into());

            let mut body = String::new();
            file.read_to_string(&mut body)?;

            let mtime = platform::file_last_modified(&file);

            script_path = std::env::current_dir()?.join(path);
            content = body;

            Input::File(&script_name, &script_path, &content, mtime)
        },
        (Some(expr), true, false) => {
            content = expr;
            Input::Expr(&content, args.template.as_ref().map(|s| &**s))
        },
        (Some(loop_), false, true) => {
            content = loop_;
            Input::Loop(&content, args.count)
        },
        (None, _, _) => Err((Blame::Human, consts::NO_ARGS_MESSAGE))?,
        _ => Err((Blame::Human,
            "cannot specify both --expr and --loop"))?
    };
    info!("input: {:?}", input);

    /*
    Sort out the dependencies.  We want to do a few things:

    - Sort them so that they hash consistently.
    - Check for duplicates.
    - Expand `pkg` into `pkg=*`.
    */
    let deps = {
        use std::collections::HashMap;
        use std::collections::hash_map::Entry::{Occupied, Vacant};

        let mut deps: HashMap<String, String> = HashMap::new();
        for dep in args.dep.iter().chain(args.dep_extern.iter()).cloned() {
            // Append '=*' if it needs it.
            let dep = match dep.find('=') {
                Some(_) => dep,
                None => dep + "=*"
            };

            let mut parts = dep.splitn(2, '=');
            let name = parts.next().expect("dependency is missing name");
            let version = parts.next().expect("dependency is missing version");
            assert!(parts.next().is_none(), "dependency somehow has three parts?!");

            if name == "" {
                Err((Blame::Human, "cannot have empty dependency package name"))?;
            }

            if version == "" {
                Err((Blame::Human, "cannot have empty dependency version"))?;
            }

            match deps.entry(name.into()) {
                Vacant(ve) => {
                    ve.insert(version.into());
                },
                Occupied(oe) => {
                    // This is *only* a problem if the versions don't match.  We won't try to do anything clever in terms of upgrading or resolving or anything... exact match or go home.
                    let existing = oe.get();
                    if &version != existing {
                        Err((Blame::Human,
                            format!("conflicting versions for dependency '{}': '{}', '{}'",
                                name, existing, version)))?;
                    }
                }
            }
        }

        // Sort and turn into a regular vec.
        let mut deps: Vec<(String, String)> = deps.into_iter().collect();
        deps.sort();
        deps
    };
    info!("deps: {:?}", deps);

    /*
    Generate the prelude items, if we need any.  Again, ensure consistent and *valid* sorting.
    */
    let prelude_items = {
        let unstable_features = args.unstable_features.iter()
            .map(|uf| format!("#![feature({})]", uf));
        let dep_externs = args.dep_extern.iter()
            .map(|d| match d.find('=') {
                Some(i) => &d[..i],
                None => &d[..]
            })
            .map(|d| match d.contains('-') {
                true => Cow::from(d.replace("-", "_")),
                false => Cow::from(d)
            })
            .map(|d| format!("#[macro_use] extern crate {};", d));

        let externs = args.extern_.iter()
            .map(|n| format!("#[macro_use] extern crate {};", n));

        let mut items: Vec<_> = unstable_features.chain(dep_externs).chain(externs).collect();
        items.sort();
        items
    };
    info!("prelude_items: {:?}", prelude_items);

    // Work out what to do.
    let action = decide_action_for(
        &input,
        deps,
        prelude_items,
        args.debug,
        args.pkg_path,
        args.gen_pkg_only,
        args.build_only,
        args.force,
        args.features,
        args.use_bincache,
        args.build_kind,
    )?;
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

    // Run it!
    if action.execute {
        fn hint<F: FnOnce(&mut Command) -> &mut Command>(f: F) -> F { f }
        let add_env = hint(move |cmd| {
            cmd.env("CARGO_SCRIPT_SCRIPT_PATH", input.path().unwrap_or(Path::new("")));
            cmd.env("CARGO_SCRIPT_SAFE_NAME", input.safe_name());
            cmd.env("CARGO_SCRIPT_PKG_NAME", input.package_name());
            cmd.env("CARGO_SCRIPT_BASE_PATH", input.base_path());
            cmd
        });

        if action.build_kind.can_exec_directly() {
            let exe_path = get_exe_path(action.build_kind, &action.pkg_path)?;
            info!("executing {:?}", exe_path);
            match {
                Command::new(exe_path)
                    .args(&args.args)
                    .chain_map(add_env)
                    .status()
                    .map(|st| st.code().unwrap_or(1))
            }? {
                0 => (),
                n => return Ok(n)
            }
        } else {
            let cmd_name = action.build_kind.exec_command();
            info!("running `cargo {}`", cmd_name);
            let mut cmd = action.cargo(cmd_name)?;
            add_env(&mut cmd);
            match cmd.status().map(|st| st.code().unwrap_or(1))? {
                0 => (),
                n => return Ok(n)
            }
        }
    }

    // If nothing else failed, I suppose we succeeded.
    Ok(0)
}

/**
Clean up the cache folder.

Looks for all folders whose metadata says they were created at least `max_age` in the past and kills them dead.
*/
fn clean_cache(max_age: u64) -> Result<()> {
    info!("cleaning cache with max_age: {:?}", max_age);

    if max_age == 0 {
        info!("max_age is 0, clearing binary cache...");
        let cache_dir = get_binary_cache_path()?;
        if ALLOW_AUTO_REMOVE {
            if let Err(err) = fs::remove_dir_all(&cache_dir) {
                error!("failed to remove binary cache {:?}: {}", cache_dir, err);
            }
        }
    }

    let cutoff = platform::current_time() - max_age;
    info!("cutoff:     {:>20?} ms", cutoff);

    let cache_dir = get_script_cache_path()?;
    for child in try!(fs::read_dir(cache_dir)) {
        let child = child?;
        let path = child.path();
        if path.is_file_polyfill() { continue }

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
                        return true
                    }
                };
                platform::file_last_modified(&meta_file)
            };
            info!("meta_mtime: {:>20?} ms", meta_mtime);

            (meta_mtime <= cutoff)
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
fn gen_pkg_and_compile(
    input: &Input,
    action: &InputAction,
) -> Result<()> {
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
            },
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
            },
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
        let mut cmd = cargo("build", &*mani_path.to_string_lossy(), action.use_bincache, &meta)?;

        #[cfg(feature="suppress-cargo-output")]
        macro_rules! get_status {
            ($cmd:expr) => {
                // `try!` doesn't work here on <=1.12.
                (match util::suppress_child_output(
                    &mut $cmd,
                    ::std::time::Duration::from_millis(CARGO_OUTPUT_TIMEOUT)
                ) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                }).status()
            }
        }

        #[cfg(not(feature="suppress-cargo-output"))]
        macro_rules! get_status {
            ($cmd:expr) => {
                $cmd.status()
            }
        }

        let compile_err = get_status!(cmd).map_err(|e| Into::<MainError>::into(e))
            .and_then(|st|
                match st.code() {
                    Some(0) => Ok(()),
                    Some(st) => Err(format!("cargo failed with status {}", st).into()),
                    None => Err("cargo failed".into())
                });

        // Drop out now if compilation failed.
        let _ = compile_err?;

        // Find out and cache what the executable was called.
        let _ = cargo_target(input, pkg_path, &*mani_path.to_string_lossy(), action.use_bincache, &meta)?;

        if action.use_bincache {
            // Write out the metadata hash to tie this executable to a particular chunk of metadata.  This is to avoid issues with multiple scripts with the same name being compiled to a common target directory.
            let meta_hash = action.metadata.sha1_hash();
            info!("writing meta hash: {:?}...", meta_hash);
            let exe_meta_hash_path = get_meta_hash_path(action.use_bincache, pkg_path)?;
            let mut f = fs::File::create(&exe_meta_hash_path)?;
            write!(&mut f, "{}", meta_hash)?;
        }
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

    /// Use shared binary cache?
    use_bincache: bool,

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

    fn cargo(&self, cmd: &str) -> Result<Command> {
        cargo(cmd, &*self.manifest_path().to_string_lossy(), self.use_bincache, &self.metadata)
    }
}

/**
The metadata here serves two purposes:

1. It records everything necessary for compilation and execution of a package.
2. It records everything that must be exactly the same in order for a cached executable to still be valid, in addition to the content hash.
*/
#[derive(Clone, Debug, Eq, PartialEq, RustcDecodable, RustcEncodable)]
struct PackageMetadata {
    /// Path to the script file.
    path: Option<String>,

    /// Last-modified timestamp for script file.
    modified: Option<u64>,

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
    debug: bool,
    pkg_path: Option<String>,
    gen_pkg_only: bool,
    build_only: bool,
    force: bool,
    features: Option<String>,
    use_bincache: Option<bool>,
    build_kind: BuildKind,
) -> Result<InputAction> {
    let (pkg_path, using_cache) = pkg_path.map(|p| (p.into(), false))
        .unwrap_or_else(|| {
            // This can't fail.  Seriously, we're *fucked* if we can't work this out.
            let cache_path = get_script_cache_path().unwrap();
            info!("cache_path: {:?}", cache_path);

            let id = {
                let deps_iter = deps.iter()
                    .map(|&(ref n, ref v)| (n as &str, v as &str));

                // Again, also fucked if we can't work this out.
                input.compute_id(deps_iter).unwrap()
            };
            info!("id: {:?}", id);

            (cache_path.join(&id), true)
        });
    info!("pkg_path: {:?}", pkg_path);
    info!("using_cache: {:?}", using_cache);

    info!("splitting input...");
    let (mani_str, script_str) = manifest::split_input(input, &deps, &prelude)?;

    // Forcibly override some flags based on build kind.
    let (debug, force, build_only) = match build_kind {
        BuildKind::Normal => (debug, force, build_only),
        BuildKind::Test => (true, false, false),
        BuildKind::Bench => (false, false, false),
    };

    // Construct input metadata.
    let input_meta = {
        let (path, mtime, template) = match *input {
            Input::File(_, path, _, mtime)
                => (Some(path.to_string_lossy().into_owned()), Some(mtime), None),
            Input::Expr(_, template)
                => (None, None, template),
            Input::Loop(..)
                => (None, None, None)
        };
        PackageMetadata {
            path: path,
            modified: mtime,
            template: template.map(Into::into),
            debug: debug,
            deps: deps,
            prelude: prelude,
            features: features,
            manifest_hash: hash_str(&mani_str),
            script_hash: hash_str(&script_str),
        }
    };
    info!("input_meta: {:?}", input_meta);

    // Lazy powers, ACTIVATE!
    let mut action = InputAction {
        compile: force,
        force_compile: force,
        emit_metadata: true,
        execute: !build_only,
        pkg_path: pkg_path,
        using_cache: using_cache,
        use_bincache: use_bincache.unwrap_or(using_cache),
        metadata: input_meta,
        old_metadata: None,
        manifest: mani_str,
        script: script_str,
        build_kind: build_kind,
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
    if gen_pkg_only {
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
            info!("recompiling because: failed to load metadata");
            debug!("get_pkg_metadata error: {}", err.description());
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

    /*
    Next test: does the executable exist at all?
    */
    let exe_exists = match get_exe_path(action.build_kind, &action.pkg_path) {
        Ok(exe_path) => exe_path.is_file_polyfill(),
        Err(_) => false,
    };
    if !exe_exists {
        info!("recompiling because: executable doesn't exist or isn't a file");
        bail!(compile: true)
    }

    /*
    Finally: check to see if `{exe_path}.meta-hash` exists and contains a hash that matches the metadata.  Yes, this is somewhat round-about, but we need to do this to account for cases where Cargo's target directory has been set to a fixed, shared location.

    Note that we *do not* do this if we aren't using the cache.
    */
    if action.use_bincache {
        let exe_meta_hash_path = get_meta_hash_path(action.use_bincache, &action.pkg_path).unwrap();
        if !exe_meta_hash_path.is_file_polyfill() {
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
    }

    // That's enough; let's just go with it.
    Ok(action)
}

/**
Figures out where the output executable for the input should be.

This *requires* that `cargo_target` has already been called on the package.
*/
fn get_exe_path<P>(build_kind: BuildKind, pkg_path: P) -> Result<PathBuf>
where P: AsRef<Path> {
    use std::fs::File;

    // We don't directly run tests and benchmarks.
    match build_kind {
        BuildKind::Normal => (),
        BuildKind::Test | BuildKind::Bench => {
            return Err("tried to get executable path for test/bench build".into());
        },
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
fn get_meta_hash_path<P>(use_bincache: bool, pkg_path: P) -> Result<PathBuf>
where P: AsRef<Path> {
    if !use_bincache {
        panic!("tried to get meta-hash path when not using binary cache");
    }
    Ok(pkg_path.as_ref().join("target.meta-hash"))
}

/**
Load the package metadata, given the path to the package's cache folder.
*/
fn get_pkg_metadata<P>(pkg_path: P) -> Result<PackageMetadata>
where P: AsRef<Path> {
    let meta_path = get_pkg_metadata_path(pkg_path);
    debug!("meta_path: {:?}", meta_path);
    let mut meta_file = fs::File::open(&meta_path)?;

    let meta_str = {
        let mut s = String::new();
        meta_file.read_to_string(&mut s).unwrap();
        s
    };
    let meta: PackageMetadata = rustc_serialize::json::decode(&meta_str)
        .map_err(|err| err.to_string())?;

    Ok(meta)
}

/**
Work out the path to a package's metadata file.
*/
fn get_pkg_metadata_path<P>(pkg_path: P) -> PathBuf
where P: AsRef<Path> {
    pkg_path.as_ref().join(consts::METADATA_FILE)
}

/**
Save the package metadata, given the path to the package's cache folder.
*/
fn write_pkg_metadata<P>(pkg_path: P, meta: &PackageMetadata) -> Result<()>
where P: AsRef<Path> {
    let meta_path = get_pkg_metadata_path(pkg_path);
    debug!("meta_path: {:?}", meta_path);
    let mut meta_file = fs::File::create(&meta_path)?;
    let meta_str = rustc_serialize::json::encode(meta)
        .map_err(|err| err.to_string())?;
    write!(&mut meta_file, "{}", meta_str)?;
    meta_file.flush()?;
    Ok(())
}

/**
Returns the path to the cache directory.
*/
fn get_script_cache_path() -> Result<PathBuf> {
    let cache_path = platform::get_cache_dir()?;
    Ok(cache_path.join("script-cache"))
}

/**
Returns the path to the binary cache directory.
*/
fn get_binary_cache_path() -> Result<PathBuf> {
    let cache_path = platform::get_cache_dir()?;
    Ok(cache_path.join("binary-cache"))
}

/**
Attempts to locate the script specified by the given path.  If the path as-given doesn't yield anything, it will try adding file extensions.
*/
fn find_script<P>(path: P) -> Option<(PathBuf, fs::File)>
where P: AsRef<Path> {
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
    File(&'a str, &'a Path, &'a str, u64),

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
    pub fn path(&self) -> Option<&Path> {
        use Input::*;

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
    pub fn safe_name(&self) -> &str {
        use Input::*;

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
                (0, '0'...'9') => {
                    r.push('_');
                    r.push(c);
                },
                (_, '0'...'9')
                | (_, 'a'...'z')
                | (_, 'A'...'Z')
                | (_, '_')
                | (_, '-')
                => {
                    r.push(c);
                },
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
            Input::File(_, path, _, _) => path.parent().expect("couldn't get parent directory for file input base path").into(),
            Input::Expr(..) | Input::Loop(..) => std::env::current_dir().expect("couldn't get current directory for input base path"),
        }
    }

    /**
    Compute the package ID for the input.  This is used as the name of the cache folder into which the Cargo package will be generated.
    */
    pub fn compute_id<'dep, DepIt>(&self, deps: DepIt) -> Result<OsString>
    where DepIt: IntoIterator<Item=(&'dep str, &'dep str)> {
        use shaman::digest::Digest;
        use shaman::sha1::Sha1;
        use Input::*;

        let hash_deps = || {
            let mut hasher = Sha1::new();
            for dep in deps {
                hasher.input_str("dep=");
                hasher.input_str(dep.0);
                hasher.input_str("=");
                hasher.input_str(dep.1);
                hasher.input_str(";");
            }
            hasher
        };

        match *self {
            File(name, path, _, _) => {
                let mut hasher = Sha1::new();

                // Hash the path to the script.
                hasher.input_str(&path.to_string_lossy());
                let mut digest = hasher.result_str();
                digest.truncate(consts::ID_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push("file-");
                id.push(name);
                id.push("-");
                id.push(if STUB_HASHES { "stub" } else { &*digest });
                Ok(id)
            },
            Expr(content, template) => {
                let mut hasher = hash_deps();

                hasher.input_str("template:");
                hasher.input_str(template.unwrap_or(""));
                hasher.input_str(";");

                hasher.input_str(&content);
                let mut digest = hasher.result_str();
                digest.truncate(consts::ID_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push("expr-");
                id.push(if STUB_HASHES { "stub" } else { &*digest });
                Ok(id)
            },
            Loop(content, count) => {
                let mut hasher = hash_deps();

                // Make sure to include the [non-]presence of the `--count` flag in the flag, since it changes the actual generated script output.
                hasher.input_str("count:");
                hasher.input_str(if count { "true;" } else { "false;" });

                hasher.input_str(&content);
                let mut digest = hasher.result_str();
                digest.truncate(consts::ID_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push("loop-");
                id.push(if STUB_HASHES { "stub" } else { &*digest });
                Ok(id)
            },
        }
    }
}

/**
Shorthand for hashing a string.
*/
fn hash_str(s: &str) -> String {
    use shaman::digest::Digest;
    use shaman::sha1::Sha1;
    let mut hasher = Sha1::new();
    hasher.input_str(s);
    hasher.result_str()
}

enum FileOverwrite {
    Same,
    Changed { new_hash: String },
}

/**
Overwrite a file if and only if the contents have changed.
*/
fn overwrite_file<P>(path: P, content: &str, hash: Option<&str>) -> Result<FileOverwrite>
where P: AsRef<Path> {
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
    Ok(FileOverwrite::Changed { new_hash: new_hash })
}

/**
Constructs a Cargo command that runs on the script package.
*/
fn cargo(cmd_name: &str, manifest: &str, use_bincache: bool, meta: &PackageMetadata) -> Result<Command> {
    let mut cmd = Command::new("cargo");
    cmd.arg(cmd_name)
        .arg("--manifest-path").arg(manifest);

    if platform::force_cargo_color() {
        cmd.arg("--color").arg("always");
    }

    if use_bincache {
        cmd.env("CARGO_TARGET_DIR", get_binary_cache_path()?);
    }

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
fn cargo_target<P>(input: &Input, pkg_path: P, manifest: &str, use_bincache: bool, meta: &PackageMetadata) -> Result<PathBuf>
where P: AsRef<Path> {
    lazy_static! {
        static ref VER_JSON_MSGS: Version = Version::parse("0.18.0").unwrap();
    }

    trace!("cargo_target(_, {:?}, {:?}, {:?}, _)", pkg_path.as_ref(), manifest, use_bincache);

    let cargo_ver = cargo_version()
        .err_tag("could not determine target filename")?;

    let mut use_guess = false;
    use_guess |= work_around_issue_50();
    use_guess |= if cargo_ver < *VER_JSON_MSGS {
        trace!(".. cargo {:?} is too old to support JSON output", cargo_ver);
        true
    } else {
        false
    };

    let exe_path = if use_guess {
        cargo_target_by_guess(input, use_bincache, pkg_path.as_ref(), meta)?
    } else {
        cargo_target_by_message(input, manifest, use_bincache, meta)?
    };

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

/**
Figures out where the output executable for the input should be by guessing.

Depending on the configuration, this might not work.  On the other hand, this actually works (usually) prior to Cargo 0.18 (Rust 1.17).
*/
fn cargo_target_by_guess(input: &Input, use_bincache: bool, pkg_path: &Path, meta: &PackageMetadata) -> Result<PathBuf> {
    trace!("cargo_target_by_guess(_, {:?}, {:?}, _)", use_bincache, pkg_path);

    let profile = match meta.debug {
        true => "debug",
        false => "release"
    };
    let target_path = if use_bincache {
        get_binary_cache_path()?
    } else {
        pkg_path.join("target")
    };
    let mut exe_path = target_path.join(profile).join(&input.package_name()).into_os_string();
    exe_path.push(std::env::consts::EXE_SUFFIX);
    Ok(exe_path.into())
}

/**
Gets the path to the package's target file by parsing the output of `cargo build`.

This only works on Cargo 0.18 (Rust 1.17) and higher.
*/
fn cargo_target_by_message(input: &Input, manifest: &str, use_bincache: bool, meta: &PackageMetadata) -> Result<PathBuf> {
    use std::io::{BufRead, BufReader};
    use rustc_serialize::json;

    trace!("cargo_target_by_message(_, {:?}, {:?}, _)", manifest, use_bincache);

    let mut cmd = cargo("build", manifest, use_bincache, meta)?;
    cmd.arg("--message-format=json");
    cmd.stdout(process::Stdio::piped());
    cmd.stderr(process::Stdio::null());

    trace!(".. cmd: {:?}", cmd);

    let mut child = cmd.spawn()?;
    match child.wait()?.code() {
        Some(0) => (),
        Some(st) => return Err(format!("could not determine target filename: cargo exited with status {}", st).into()),
        None => return Err(format!("could not determine target filename: cargo exited abnormally").into()),
    }

    let mut line = String::with_capacity(1024);
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let null = json::Json::Null;
    let package_name = input.package_name();

    let mut line_num = 0;
    loop {
        line_num += 1;
        line.clear();
        let bytes = stdout.read_line(&mut line)?;
        trace!(".. line {}, {}b: {:?}", line_num, bytes, line);
        if bytes == 0 {
            return Err("could not determine target filename: did not find appropriate cargo message".into());
        }

        let msg = json::Json::from_str(line.trim())
            .map_err(Box::new)?;

        // Is this the message we're looking for?
        if msg.find("reason").unwrap_or(&null).as_string() != Some("compiler-artifact") {
            trace!("   couldn't find `compiler-artifact`");
            continue;
        }
        if msg.find_path(&["target", "name"]).unwrap_or(&null).as_string() != Some(&package_name) {
            trace!("   couldn't find `target.name`, or it wasn't {:?}", package_name);
            continue;
        }

        // Looks like it; grab the path.
        let exe_path = msg.find_path(&["filenames"])
            .expect("could not find `filenames` in json message")
            .as_array()
            .expect("`filenames` in json message was not an array")
            [0]
            .as_string()
            .expect("`filenames[0]` in json message was not a string");

        return Ok(exe_path.into());
    }
}

/**
Get the version of the currently active cargo.
*/
fn cargo_version() -> Result<Version> {
    use regex::Regex;

    lazy_static! {
        static ref RE_VERSION: Regex = Regex::new(r#"^cargo[ -](\S+)"#).unwrap();
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("-V");

    let child = cmd.output()?;
    match child.status.code() {
        Some(0) => (),
        Some(st) => return Err(format!("could not determine cargo version: cargo exited with status {}", st).into()),
        None => return Err(format!("could not determine cargo version: cargo exited abnormally").into()),
    }

    let stdout = String::from_utf8_lossy(&child.stdout);
    let m = match RE_VERSION.captures(&stdout) {
        Some(m) => m,
        None => return Err(format!("could not determine cargo version: output did not match expected").into()),
    };

    let ver = m.get(1).unwrap();
    Ok(Version::parse(ver.as_str())
        .map_err(Box::new)?)
}

/**
Do we need to work around [issue #50](https://github.com/DanielKeep/cargo-script/issues/50)?

Sometimes, `cargo-script` will hang when trying to read the JSON output of `cargo build`.
*/
fn work_around_issue_50() -> bool {
    let suffers = cfg!(issue_50);
    let ignored = std::env::var_os("CARGO_SCRIPT_IGNORE_ISSUE_50").is_some();
    match (suffers, ignored) {
        (true, true) => { trace!(".. issue 50 relevant, but ignored"); false },
        (true, false) => { trace!(".. working around issue 50"); true },
        (false, _) => { false },
    }
}
