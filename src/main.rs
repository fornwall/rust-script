#![forbid(unsafe_code)]

mod arguments;
mod build_kind;
mod consts;
mod defer;
mod error;
mod manifest;
mod platform;
mod templates;

#[cfg(windows)]
mod file_assoc;

#[cfg(not(windows))]
mod file_assoc {}

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use arguments::Args;
use log::{debug, error, info};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::build_kind::BuildKind;
use crate::defer::Defer;
use crate::error::{MainError, MainResult};
use sha1::{Digest, Sha1};

fn main() {
    env_logger::init();

    match try_main() {
        Ok(code) => {
            std::process::exit(code);
        }
        Err(err) => {
            eprintln!("error: {}", err);
            std::process::exit(1);
        }
    }
}

fn try_main() -> MainResult<i32> {
    let args = arguments::Args::parse();
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
            println!("rust-script cache cleared.");
            return Ok(0);
        }
    }

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

            if name.is_empty() {
                return Err(("cannot have empty dependency package name").into());
            } else if version.is_empty() {
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

    let input = match (args.script.clone().unwrap(), args.expr, args.loop_) {
        (script, false, false) => {
            let (path, mut file) =
                find_script(script.as_ref()).ok_or(format!("could not find script: {}", script))?;

            let script_name = path
                .file_stem()
                .map(|os| os.to_string_lossy().into_owned())
                .unwrap_or_else(|| "unknown".into());

            let mut body = String::new();
            file.read_to_string(&mut body)?;

            let script_path = std::env::current_dir()?.join(path);

            let base_path = if let Some(base_path_arg) = &args.base_path {
                Path::new(base_path_arg).into()
            } else {
                script_path
                    .parent()
                    .expect("couldn't get parent directory for file input base path")
                    .into()
            };

            Input::File(script_name, script_path, body, base_path)
        }
        (expr, true, false) => {
            let base_path = if let Some(base_path_arg) = &args.base_path {
                Path::new(base_path_arg).into()
            } else {
                std::env::current_dir().expect("couldn't get current directory for input base path")
            };
            Input::Expr(expr, base_path)
        }
        (loop_, false, true) => {
            let base_path = if let Some(base_path_arg) = &args.base_path {
                Path::new(base_path_arg).into()
            } else {
                std::env::current_dir().expect("couldn't get current directory for input base path")
            };
            Input::Loop(loop_, args.count, base_path)
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

    generate_package(&action)?;

    // Once we're done, clean out old packages from the cache.
    let _defer_clear = {
        Defer::<_, MainError>::new(move || {
            if args.clear_cache {
                // Do nothing if cache was cleared explicitly.
            } else {
                clean_cache(consts::MAX_CACHE_AGE_MS)?;
            }
            Ok(())
        })
    };

    if !action.execute {
        println!("{}", action.pkg_path.display());
        return Ok(0);
    }

    let mut cmd = action.command_to_execute(&args.script_args, args.wrapper)?;
    #[cfg(unix)]
    {
        let err = cmd.exec();
        Err(MainError::from(err))
    }
    #[cfg(not(unix))]
    {
        let exit_code = cmd.status().map(|st| st.code().unwrap_or(1))?;
        Ok(exit_code)
    }
}

/**
Clean up the cache folder.

Looks for all folders whose metadata says they were created at least `max_age` in the past and kills them dead.
*/
fn clean_cache(max_age: u128) -> MainResult<()> {
    info!("cleaning cache with max_age: {:?}", max_age);

    if max_age == 0 {
        info!("max_age is 0, clearing binary cache...");
        let cache_dir = platform::binary_cache_path();
        if let Err(err) = fs::remove_dir_all(&cache_dir) {
            error!("failed to remove binary cache {:?}: {}", cache_dir, err);
        }
    }

    let cutoff = platform::current_time() - max_age;
    info!("cutoff:     {:>20?} ms", cutoff);

    let cache_dir = platform::generated_projects_cache_path();
    for child in fs::read_dir(cache_dir)? {
        let child = child?;
        let path = child.path();
        if path.is_file() {
            continue;
        }

        info!("checking: {:?}", path);

        let remove_dir = || {
            let meta_mtime = platform::dir_last_modified(&child);
            info!("meta_mtime: {:>20?} ms", meta_mtime);

            meta_mtime <= cutoff
        };

        if remove_dir() {
            info!("removing {:?}", path);
            if let Err(err) = fs::remove_dir_all(&path) {
                error!("failed to remove {:?} from cache: {}", path, err);
            }
        }
    }
    info!("done cleaning cache.");
    Ok(())
}

// Generate a package from the input.
fn generate_package(action: &InputAction) -> MainResult<()> {
    info!("creating pkg dir...");
    fs::create_dir_all(&action.pkg_path)?;
    let cleanup_dir: Defer<_, MainError> = Defer::new(|| {
        if action.using_cache {
            // Only cleanup on failure if we are using the shared package
            // cache, and not when the user has specified the package path
            // (since that would risk removing user files).
            info!("cleaning up cache directory {:?}", &action.pkg_path);
            fs::remove_dir_all(&action.pkg_path)?;
        }
        Ok(())
    });

    info!("generating Cargo package...");
    let mani_path = action.manifest_path();

    overwrite_file(&mani_path, &action.manifest)?;
    if let Some(script) = &action.script {
        overwrite_file(&action.script_path, script)?;
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
    /// Always show cargo output?
    cargo_output: bool,

    /**
    Force Cargo to do a recompile, even if it thinks it doesn't have to.

    `compile` must be `true` for this to have any effect.
    */
    force_compile: bool,

    /// Execute the compiled binary?
    execute: bool,

    /// Directory where the package should live.
    pkg_path: PathBuf,

    /// Path of the source code that Cargo.toml refers.
    script_path: PathBuf,

    /**
    Is the package directory in the cache?

    Currently, this can be inferred from `emit_metadata`, but there's no *intrinsic* reason they should be tied together.
    */
    using_cache: bool,

    /**
    Which toolchain the script should be built with.

    `None` indicates that the script should be built with a stable toolchain.
    */
    toolchain_version: Option<String>,

    /// If script should be built in debug mode.
    debug: bool,

    /// The package manifest contents.
    manifest: String,

    /// The script source in case it has to be written.
    script: Option<String>,

    /// Did the user ask to run tests or benchmarks?
    build_kind: BuildKind,

    // Name of the built binary
    bin_name: String,

    // How the script was called originally
    #[cfg(unix)]
    original_script_path: Option<String>,
}

impl InputAction {
    fn manifest_path(&self) -> PathBuf {
        self.pkg_path.join("Cargo.toml")
    }

    fn command_to_execute(
        &self,
        script_args: &[String],
        wrapper: Option<String>,
    ) -> MainResult<Command> {
        let release_mode = !self.debug && !matches!(self.build_kind, BuildKind::Bench);

        let built_binary_path = platform::binary_cache_path()
            .join(if release_mode { "release" } else { "debug" })
            .join({
                #[cfg(windows)]
                {
                    format!("{}.exe", &self.bin_name)
                }
                #[cfg(not(windows))]
                {
                    &self.bin_name
                }
            });

        let manifest_path = self.manifest_path();

        let execute_command = || {
            if let Some(wrapper) = wrapper {
                let wrapper_words = shell_words::split(&wrapper).unwrap();
                if wrapper_words.is_empty() {
                    return MainResult::Err(MainError::OtherBorrowed(
                        "The wrapper cannot be empty",
                    ));
                }
                let mut cmd = Command::new(&wrapper_words[0]);
                if wrapper_words.len() > 1 {
                    cmd.args(wrapper_words[1..].iter());
                }
                cmd.arg(&built_binary_path);
                cmd.args(script_args.iter());
                Ok(cmd)
            } else {
                let mut cmd = Command::new(&built_binary_path);
                #[cfg(unix)]
                if let Some(original_script_path) = &self.original_script_path {
                    cmd.arg0(original_script_path);
                }
                cmd.args(script_args.iter());
                Ok(cmd)
            }
        };

        if matches!(self.build_kind, BuildKind::Normal) && !self.force_compile {
            match fs::File::open(&built_binary_path) {
                Ok(built_binary_file) => {
                    // When possible, use creation time instead of modified time as cargo may copy
                    // an already built binary (with old modified time):
                    let built_binary_time = built_binary_file
                        .metadata()?
                        .created()
                        .unwrap_or(built_binary_file.metadata()?.modified()?);
                    match (
                        fs::File::open(&self.script_path),
                        fs::File::open(manifest_path),
                    ) {
                        (Ok(script_file), Ok(manifest_file)) => {
                            let script_mtime = script_file.metadata()?.modified()?;
                            let manifest_mtime = manifest_file.metadata()?.modified()?;
                            if built_binary_time.cmp(&script_mtime).is_ge()
                                && built_binary_time.cmp(&manifest_mtime).is_ge()
                            {
                                debug!("Keeping old binary");
                                return execute_command();
                            } else {
                                debug!("Old binary too old - rebuilding");
                            }
                        }
                        (Err(error), _) | (_, Err(error)) => {
                            return Err(error::MainError::Io(error));
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    debug!("No old binary found");
                }
                Err(e) => {
                    return Err(error::MainError::Io(e));
                }
            }
        }

        let maybe_toolchain_version = self.toolchain_version.as_deref();

        let mut cmd = Command::new("cargo");
        if let Some(toolchain_version) = maybe_toolchain_version {
            cmd.arg(format!("+{}", toolchain_version));
        }
        cmd.arg(self.build_kind.exec_command());

        if matches!(self.build_kind, BuildKind::Normal) && !self.cargo_output {
            cmd.arg("-q");
        }

        cmd.current_dir(&self.pkg_path);

        if platform::force_cargo_color() {
            cmd.arg("--color").arg("always");
        }

        let cargo_target_dir = format!("{}", platform::binary_cache_path().display(),);
        cmd.arg("--target-dir");
        cmd.arg(cargo_target_dir);

        if release_mode {
            cmd.arg("--release");
        }

        if matches!(self.build_kind, BuildKind::Normal) {
            if cmd.status()?.code() == Some(0) {
                cmd = execute_command()?;
            } else {
                return Err(MainError::OtherOwned("Could not execute cargo".to_string()));
            }
        } else {
            cmd.args(script_args.iter());
        }

        Ok(cmd)
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
    let input_id = {
        let deps_iter = deps.iter().map(|(n, v)| (n as &str, v as &str));
        input.compute_id(deps_iter)
    };
    info!("id: {:?}", input_id);

    let pkg_name = input.package_name();
    let bin_name = format!("{}_{}", &*pkg_name, input_id.to_str().unwrap());

    let (pkg_path, using_cache) = args
        .pkg_path
        .as_ref()
        .map(|p| (p.into(), false))
        .unwrap_or_else(|| {
            let cache_path = platform::generated_projects_cache_path();
            (cache_path.join(&input_id), true)
        });
    info!("pkg_path: {:?}", pkg_path);
    info!("using_cache: {:?}", using_cache);

    let toolchain_version = args
        .toolchain_version
        .clone()
        .or_else(|| match args.build_kind {
            BuildKind::Bench => Some("nightly".into()),
            _ => None,
        });

    let script_name = format!("{}.rs", input.safe_name());

    let (mani_str, script_path, script_str) = manifest::split_input(
        input,
        input.base_path(),
        &deps,
        &prelude,
        &pkg_path,
        &bin_name,
        &script_name,
        toolchain_version.clone(),
    )?;

    // Forcibly override some flags based on build kind.
    let debug = match args.build_kind {
        BuildKind::Normal => args.debug,
        BuildKind::Test => true,
        BuildKind::Bench => false,
    };

    Ok(InputAction {
        cargo_output: args.cargo_output,
        force_compile: args.force,
        execute: !args.gen_pkg_only,
        pkg_path,
        script_path,
        using_cache,
        toolchain_version,
        debug,
        manifest: mani_str,
        script: script_str,
        build_kind: args.build_kind,
        bin_name,
        #[cfg(unix)]
        original_script_path: args.script.clone(),
    })
}

/// Attempts to locate the script specified by the given path.
fn find_script(path: &Path) -> Option<(PathBuf, fs::File)> {
    if let Ok(file) = fs::File::open(path) {
        return Some((path.into(), file));
    }

    if path.extension().is_none() {
        for &ext in &["ers", "rs"] {
            let path = path.with_extension(ext);
            if let Ok(file) = fs::File::open(&path) {
                return Some((path, file));
            }
        }
    }

    None
}

/**
Represents an input source for a script.
*/
#[derive(Clone, Debug)]
pub enum Input {
    /**
    The input is a script file.

    The tuple members are: the name, absolute path, script contents, base path.
    */
    File(String, PathBuf, String, PathBuf),

    /**
    The input is an expression.

    The tuple member is: the script contents, base path.
    */
    Expr(String, PathBuf),

    /**
    The input is a loop expression.

    The tuple member is: the script contents, whether the `--count` flag was given, base path.
    */
    Loop(String, bool, PathBuf),
}

impl Input {
    /**
    Return the path to the script, if it has one.
    */
    pub fn path(&self) -> Option<&Path> {
        use crate::Input::*;

        match self {
            File(_, path, _, _) => Some(path),
            Expr(..) => None,
            Loop(..) => None,
        }
    }

    /**
    Return the "safe name" for the input.  This should be filename-safe.

    Currently, nothing is done to ensure this, other than hoping *really hard* that we don't get fed some excessively bizarre input filename.
    */
    pub fn safe_name(&self) -> &str {
        use crate::Input::*;

        match self {
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
                (_, '0'..='9') | (_, 'a'..='z') | (_, '_') | (_, '-') => {
                    r.push(c);
                }
                (_, 'A'..='Z') => {
                    // Convert uppercase characters to lowercase to avoid `non_snake_case` warnings.
                    r.push(c.to_ascii_lowercase());
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
    pub fn base_path(&self) -> &PathBuf {
        match self {
            Input::File(_, _, _, base_path)
            | Input::Expr(_, base_path)
            | Input::Loop(_, _, base_path) => base_path,
        }
    }

    // Compute the package ID for the input.
    // This is used as the name of the cache folder into which the Cargo package
    // will be generated.
    pub fn compute_id<'dep, DepIt>(&self, deps: DepIt) -> OsString
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

        match self {
            File(_, path, _, _) => {
                let mut hasher = Sha1::new();

                // Hash the path to the script.
                hasher.update(&*path.to_string_lossy());
                let mut digest = format!("{:x}", hasher.finalize());
                digest.truncate(consts::ID_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push(&*digest);
                id
            }
            Expr(content, _) => {
                let mut hasher = hash_deps();

                hasher.update(content);
                let mut digest = format!("{:x}", hasher.finalize());
                digest.truncate(consts::ID_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push(&*digest);
                id
            }
            Loop(content, count, _) => {
                let mut hasher = hash_deps();

                // Make sure to include the [non-]presence of the `--count` flag in the flag, since it changes the actual generated script output.
                hasher.update("count:");
                hasher.update(if *count { "true;" } else { "false;" });

                hasher.update(content);
                let mut digest = format!("{:x}", hasher.finalize());
                digest.truncate(consts::ID_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push(&*digest);
                id
            }
        }
    }
}

// Overwrite a file if and only if the contents have changed.
fn overwrite_file(path: &Path, content: &str) -> MainResult<()> {
    debug!("overwrite_file({:?}, _)", path);
    let mut existing_content = String::new();
    match fs::File::open(path) {
        Ok(mut file) => {
            file.read_to_string(&mut existing_content)?;
            if existing_content == content {
                debug!("Equal content");
                return Ok(());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Continue
        }
        Err(e) => {
            return Err(error::MainError::Io(e));
        }
    }

    debug!(".. files differ");
    let dir = path.parent().ok_or("The given path should be a file")?;
    let mut temp_file = tempfile::NamedTempFile::new_in(dir)?;
    temp_file.write_all(content.as_bytes())?;
    temp_file.flush()?;
    temp_file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn test_package_name() {
    let input = Input::File(
        "Script".to_string(),
        Path::new("path").into(),
        "script".to_string(),
        Path::new("path").into(),
    );
    assert_eq!("script", input.package_name());
    let input = Input::File(
        "1Script".to_string(),
        Path::new("path").into(),
        "script".to_string(),
        Path::new("path").into(),
    );
    assert_eq!("_1script", input.package_name());
}
