#![allow(deprecated)] // for file metadata
#![feature(collections)]
#![feature(fs_time)]
#![feature(path_ext)]

extern crate docopt;
extern crate env_logger;
extern crate flate2;
#[macro_use] extern crate log;
extern crate rustc_serialize;
extern crate shaman;
extern crate toml;

const STUB_HASHES: bool = false;

mod error;

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;

use error::{Blame, MainError};

type Result<T> = std::result::Result<T, MainError>;

#[derive(Debug, RustcDecodable)]
struct Args {
    arg_script: Option<String>,

    flag_expr: Option<String>,
    flag_loop: Option<String>,
    flag_count: bool,

    flag_build_only: bool,
    flag_debug: bool,
    flag_dep: Vec<String>,
    flag_force: bool,
}

const USAGE: &'static str = "Usage:
    cargo script [options] [--dep SPEC...] <script>
    cargo script [options] [--dep SPEC...] --expr EXPR
    cargo script [options] [--dep SPEC...] [--count] --loop CLOSURE
    cargo script --help

Options:
    -h, --help              Show this message.

    --expr EXPR             Evaluate an expression and display the result.
    --loop CLOSURE          Invoke a closure once for each line from standard input.
    --count                 Invoke the loop closure with two arguments: line, line_number.

    --build-only            Build the script, but don't run it.
    --debug                 Build a debug executable rather than an optimised one.
    --dep SPEC              Add an additional Cargo dependency.  Each SPEC can be either just the package name (which will assume the latest version) or a full `name=version` spec.
    --force                 Force the script to be rebuilt.
";

fn main() {
    env_logger::init().unwrap();
    info!("starting");
    match try_main() {
        Ok(0) => (),
        Ok(code) => {
            std::process::exit(code);
        },
        Err(ref err) if err.is_human() => {
            // TODO: output to stderr.
            println!("Error: {}", err);
            std::process::exit(1);
        },
        result @ Err(..) => {
            result.unwrap();
        }
    }
}

fn try_main() -> Result<i32> {
    let args: Args = docopt::Docopt::new(USAGE)
        .and_then(|d| d.decode())
        .unwrap_or_else(|e| e.exit());
    info!("Arguments: {:?}", args);

    // Take the arguments and work out what our input is going to be.  Primarily, this gives us the content, a user-friendly name, and a cache-friendly ID.
    // These three are just storage for the borrows we'll actually use.
    let script_name: String;
    let script_path: PathBuf;
    let content: String;

    let input = match (args.arg_script.as_ref(), args.flag_expr, args.flag_loop) {
        (Some(script), None, None) => {
            let (path, mut file) = try!(find_script(script).ok_or("could not find script"));

            script_name = path.file_stem()
                .map(|os| os.to_string_lossy().into_owned())
                .unwrap_or("unknown".into());

            let mut body = String::new();
            try!(file.read_to_string(&mut body));

            let mtime = file.metadata().map(|md| md.modified()).unwrap_or(0);

            script_path = try!(std::env::current_dir()).join(path);
            content = body;

            Input::File(&script_name, &script_path, &content, mtime)
        },
        (None, Some(expr), None) => {
            content = expr;
            Input::Expr(&content)
        },
        (None, None, Some(loop_)) => {
            content = loop_;
            Input::Loop(&content, args.flag_count)
        },
        _ => try!(Err((Blame::Human,
            "cannot specify more than one of <script>, --expr, or --loop")))
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
        for dep in args.flag_dep {
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
                try!(Err((Blame::Human, "cannot have empty dependency package name")));
            }

            if version == "" {
                try!(Err((Blame::Human, "cannot have empty dependency version")));
            }

            match deps.entry(name.into()) {
                Vacant(ve) => {
                    ve.insert(version.into());
                },
                Occupied(oe) => {
                    // This is *only* a problem if the versions don't match.  We won't try to do anything clever in terms of upgrading or resolving or anything... exact match or go home.
                    let existing = oe.get();
                    if &version != existing {
                        try!(Err((Blame::Human,
                            format!("conflicting versions for dependency '{}': '{}', '{}'",
                                name, existing, version))));
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

    // Work out what to do.
    let (action, pkg_path, meta) = cache_action_for(&input, args.flag_debug, deps);
    info!("action: {:?}", action);
    info!("pkg_path: {:?}", pkg_path);
    info!("meta: {:?}", meta);

    // Compile if we need it.
    if action == CacheAction::Compile || args.flag_force {
        info!("compiling...");
        try!(compile(&input, &meta, &pkg_path));
    }

    // Run it!
    let exe_path = get_exe_path(&input, &pkg_path, &meta);
    info!("executing {:?}", exe_path);
    Ok(try!(Command::new(exe_path).status()
        .map(|st| st.code().unwrap_or(1))))
}

/**
Compile a package from the input.

Why take `PackageMetadata`?  To ensure that any information we need to depend on for compilation *first* passes through `cache_action_for` *and* is less likely to not be serialised with the rest of the metadata.
*/
fn compile<P>(input: &Input, meta: &PackageMetadata, pkg_path: P) -> Result<()>
where P: AsRef<Path> {
    let pkg_path = pkg_path.as_ref();

    let (mani_str, script_str) = try!(split_input(input, &meta.deps));

    try!(fs::create_dir_all(pkg_path));

    let mani_path = {
        let mani_path = pkg_path.join("Cargo.toml");
        let mut mani_f = try!(fs::File::create(&mani_path));
        try!(write!(&mut mani_f, "{}", mani_str));
        try!(mani_f.flush());
        mani_path
    };

    {
        let script_path = pkg_path.join(input.safe_name()).with_extension("rs");
        let mut script_f = try!(fs::File::create(script_path));
        try!(write!(&mut script_f, "{}", script_str));
        try!(script_f.flush());
    }

    // *bursts through wall* It's Cargo Time!
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(&*mani_path.to_string_lossy());

    if !meta.debug {
        cmd.arg("--release");
    }

    try!(cmd.status().map_err(|e| Into::<MainError>::into(e)).and_then(|st|
        match st.code() {
            Some(0) => Ok(()),
            Some(st) => Err(format!("cargo failed with status {}", st).into()),
            None => Err("cargo failed".into())
        }));

    // Write out metadata *now*.  Remember that we check the timestamp in the metadata, *not* on the executable.
    try!(write_pkg_metadata(pkg_path, meta));

    Ok(())
}

const FILE_TEMPLATE: &'static str = r#"%%"#;

const EXPR_TEMPLATE: &'static str = r#"
fn main() {
    println!("{}", (%%));
}
"#;

const LOOP_TEMPLATE: &'static str = r#"
use std::io::prelude::*;

fn main() {
    let mut out_buffer: Vec<u8> = vec![];
    let mut line_buffer = String::new();
    let mut stdin = std::io::stdin();
    loop {
        line_buffer.clear();
        let read_res = stdin.read_line(&mut line_buffer).unwrap_or(0);
        if read_res == 0 { break }
        let output = invoke_closure(&line_buffer, %%);

        out_buffer.clear();
        write!(&mut out_buffer, "{:?}", output).unwrap();
        let out_str = String::from_utf8_lossy(&out_buffer);
        if &*out_str != "()" {
            println!("{}", out_str);
        }
    }
}

fn invoke_closure<F, T>(line: &str, mut closure: F) -> T
where F: FnMut(&str) -> T {
    closure(line)
}
"#;

const LOOP_COUNT_TEMPLATE: &'static str = r#"
use std::io::prelude::*;

fn main() {
    let mut out_buffer: Vec<u8> = vec![];
    let mut line_buffer = String::new();
    let mut stdin = std::io::stdin();
    let mut count = 0;
    loop {
        line_buffer.clear();
        let read_res = stdin.read_line(&mut line_buffer).unwrap_or(0);
        if read_res == 0 { break }
        count += 1;
        let output = invoke_closure(&line_buffer, count, %%);

        out_buffer.clear();
        write!(&mut out_buffer, "{:?}", output).unwrap();
        let out_str = String::from_utf8_lossy(&out_buffer);
        if &*out_str != "()" {
            println!("{}", out_str);
        }
    }
}

fn invoke_closure<F, T>(line: &str, count: usize, mut closure: F) -> T
where F: FnMut(&str, usize) -> T {
    closure(line, count)
}
"#;

/**
Splits input into a complete Cargo manifest and unadultered Rust source.
*/
fn split_input(input: &Input, deps: &[(String, String)]) -> Result<(String, String)> {
    // First up, we need to parse any partial manifest embedded in the content.
    let (part_mani, source, template) = match *input {
        Input::File(_, _, content, _) => {
            let mut lines = content.lines_any().peekable();

            let skip = if let Some(line) = lines.peek() {
                if line.starts_with("#!") && !line.starts_with("#![") {
                    // This is a hashbang; toss it.
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if skip { lines.next(); }

            let mut manifest_end = None;
            let mut source_start = None;

            for line in lines {
                // Did we get a dash separator?
                let mut dashes = 0;
                if line.chars().all(|c| {
                    if c == '-' { dashes += 1 }
                    c.is_whitespace() || c == '-'
                }) && dashes >= 3 {
                    info!("splitting because of dash divider in line {:?}", line);
                    manifest_end = Some(&line[0..0]);
                    source_start = Some(&line[line.len()..]);
                    break;
                }

                // Ok, it's-a guessin' time!  Yes, this is *evil*.
                const SPLIT_MARKERS: &'static [&'static str] = &[
                    "//", "/*", "#![", "#[", "pub",
                    "extern", "use", "mod", "type",
                    "struct", "enum", "fn", "impl",
                    "static", "const",
                ];

                let line_trimmed = line.trim_left();

                for marker in SPLIT_MARKERS {
                    if line_trimmed.starts_with(marker) {
                        info!("splitting because of marker '{:?}'", marker);
                        manifest_end = Some(&line[0..]);
                        source_start = Some(&line[0..]);
                        break;
                    }
                }
            }

            let (manifest, source) = match (manifest_end, source_start) {
                (Some(me), Some(ss)) => {
                    (&content[..content.subslice_offset(me)],
                        &content[content.subslice_offset(ss)..])
                },
                _ => try!(Err("could not locate start of Rust source in script"))
            };

            // Hooray!
            (manifest, source, FILE_TEMPLATE)
        },
        Input::Expr(content) => ("", content, EXPR_TEMPLATE),
        Input::Loop(content, count) => {
            ("", content, if count { LOOP_COUNT_TEMPLATE } else { LOOP_TEMPLATE })
        },
    };

    let source = template.replace("%%", source);

    info!("part_mani: {:?}", part_mani);
    info!("source: {:?}", source);

    let part_mani = try!(toml::Parser::new(part_mani).parse()
        .ok_or("could not parse embedded manifest"));
    info!("part_mani: {:?}", part_mani);

    // It's-a mergin' time!
    let def_mani = try!(default_manifest(input));
    let dep_mani = try!(deps_manifest(deps));

    let mani = try!(merge_manifest(def_mani, part_mani));
    let mani = try!(merge_manifest(mani, dep_mani));
    info!("mani: {:?}", mani);

    let mani_str = format!("{}", toml::Value::Table(mani));
    info!("mani_str: {}", mani_str);

    Ok((mani_str, source))
}

const DEFAULT_MANIFEST: &'static str = concat!(r#"
[package]
name = "%n"
version = "0.1.0"
authors = ["Anonymous"]

[[bin]]
name = "%n"
path = "%n.rs"
"#);

fn default_manifest(input: &Input) -> Result<toml::Table> {
    let mani_str = DEFAULT_MANIFEST.replace("%n", input.safe_name());
    toml::Parser::new(&mani_str).parse()
        .ok_or("could not parse default manifest, somehow".into())
}

fn deps_manifest(deps: &[(String, String)]) -> Result<toml::Table> {
    let mut mani_str = String::new();
    mani_str.push_str("[dependencies]\n");

    for &(ref name, ref ver) in deps {
        mani_str.push_str(name);
        mani_str.push_str("=");

        // We only want to quote the version if it *isn't* a table.
        let quotes = match ver.starts_with("{") { true => "", false => "\"" };
        mani_str.push_str(quotes);
        mani_str.push_str(ver);
        mani_str.push_str(quotes);
        mani_str.push_str("\n");
    }

    toml::Parser::new(&mani_str).parse()
        .ok_or("could not parse dependency manifest".into())
}

fn merge_manifest(mut into_t: toml::Table, from_t: toml::Table) -> Result<toml::Table> {
    // How we're going to do this: at the top level, we will outright replace anything that isn't a table.  The *contents* of all top-level tables will be replaced.  That *should* suffice.
    for (k, v) in from_t {
        match v {
            toml::Value::Table(from_t) => {
                use std::collections::btree_map::Entry::*;

                // Merge.
                match into_t.entry(k) {
                    Vacant(e) => {
                        e.insert(toml::Value::Table(from_t));
                    },
                    Occupied(e) => {
                        let into_t = try!(as_table_mut(e.into_mut())
                            .ok_or((Blame::Human, "cannot merge manifests: cannot merge \
                                table and non-table values")));
                        into_t.extend(from_t);
                    }
                }
            },
            v => {
                // Just replace.
                into_t.insert(k, v);
            },
        }
    }

    return Ok(into_t);

    fn as_table_mut(t: &mut toml::Value) -> Option<&mut toml::Table> {
        match *t {
            toml::Value::Table(ref mut t) => Some(t),
            _ => None
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CacheAction {
    Compile,
    Execute,
}

#[derive(Clone, Debug, Eq, PartialEq, RustcDecodable, RustcEncodable)]
struct PackageMetadata {
    /// Path to the script file.
    path: Option<String>,

    /// Last-modified timestamp for script file.
    modified: Option<u64>,

    /// Was the script compiled in debug mode?
    debug: bool,

    /// Sorted list of dependencies.
    deps: Vec<(String, String)>,
}

/**
Determines whether, for a given input, a binary must be compiled, *or* an existing one can be run.
*/
fn cache_action_for(input: &Input, debug: bool, deps: Vec<(String, String)>) -> (CacheAction, PathBuf, PackageMetadata) {
    use std::fs::PathExt;

    // This can't fail.  Seriously, we're *fucked* if we can't work this out.
    let cache_path = get_cache_path().unwrap();
    info!("cache_path: {:?}", cache_path);

    let id = {
        let deps_iter = deps.iter()
            .map(|&(ref n, ref v)| (n as &str, v as &str));

        // Again, also fucked if we can't work this out.
        input.compute_id(deps_iter).unwrap()
    };
    info!("id: {:?}", id);

    let pkg_path = cache_path.join(&id);
    info!("pkg_path: {:?}", pkg_path);

    // Construct input metadata.
    let input_meta = {
        let (path, mtime) = match *input {
            Input::File(_, path, _, mtime)
                => (Some(path.to_string_lossy().into_owned()), Some(mtime)),
            Input::Expr(..)
            | Input::Loop(..)
                => (None, None)
        };
        PackageMetadata {
            path: path,
            modified: mtime,
            debug: debug,
            deps: deps,
        }
    };
    info!("input_meta: {:?}", input_meta);

    macro_rules! bail {
        () => {
            return (CacheAction::Compile, pkg_path, input_meta)
        }
    }

    let cache_meta = match get_pkg_metadata(&pkg_path) {
        Ok(meta) => meta,
        Err(err) => {
            info!("recompiling because: failed to load metadata");
            debug!("get_pkg_metadata error: {}", err.description());
            bail!()
        }
    };

    if cache_meta != input_meta {
        info!("recompiling because: metadata did not match");
        debug!("input metadata: {:?}", input_meta);
        debug!("cache metadata: {:?}", cache_meta);
        bail!()
    }

    // Next test: does the executable exist at all?
    let exe_path = get_exe_path(input, &pkg_path, &input_meta);
    if !exe_path.is_file() {
        info!("recompiling because: executable doesn't exist or isn't a file");
        bail!()
    }

    // That's enough; let's just go with it.
    (CacheAction::Execute, pkg_path, input_meta)
}

fn get_exe_path<P>(input: &Input, pkg_path: P, meta: &PackageMetadata) -> PathBuf
where P: AsRef<Path> {
    let profile = match meta.debug {
        true => "debug",
        false => "release"
    };
    let mut exe_path = pkg_path.as_ref().join("target").join(profile).join(&input.safe_name()).into_os_string();
    exe_path.push(std::env::consts::EXE_SUFFIX);
    exe_path.into()
}

const METADATA_FILE: &'static str = "metadata.json";

fn get_pkg_metadata<P>(pkg_path: P) -> Result<PackageMetadata>
where P: AsRef<Path> {
    let meta_path = pkg_path.as_ref().join(METADATA_FILE);
    debug!("meta_path: {:?}", meta_path);
    let mut meta_file = try!(fs::File::open(&meta_path));

    let meta_str = {
        let mut s = String::new();
        meta_file.read_to_string(&mut s).unwrap();
        s
    };
    let meta: PackageMetadata = try!(rustc_serialize::json::decode(&meta_str)
        .map_err(|err| err.to_string()));

    Ok(meta)
}

fn write_pkg_metadata<P>(pkg_path: P, meta: &PackageMetadata) -> Result<()>
where P: AsRef<Path> {
    let meta_path = pkg_path.as_ref().join(METADATA_FILE);
    debug!("meta_path: {:?}", meta_path);
    let mut meta_file = try!(fs::File::create(&meta_path));
    let meta_str = try!(rustc_serialize::json::encode(meta)
        .map_err(|err| err.to_string()));
    try!(write!(&mut meta_file, "{}", meta_str));
    try!(meta_file.flush());
    Ok(())
}

/**
Returns the path to the cache directory.

On Windows, LocalAppData is where user- and machine- specific data should go.  It *might* be more appropriate to use whatever the official name for "Program Data" is, though.
*/
fn get_cache_path() -> Result<PathBuf> {
    let lad_path = try!(win32::SHGetKnownFolderPath(
        &win32::FOLDERID_LocalAppData, 0, std::ptr::null_mut()));
    Ok(Path::new(&lad_path).to_path_buf().join("Cargo").join("script-cache"))
}

const SEARCH_EXTS: &'static [&'static str] = &["crs", "rs"];

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
    for &ext in SEARCH_EXTS {
        let path = path.with_extension(ext);
        if let Ok(file) = fs::File::open(&path) {
            return Some((path, file));
        }
    }

    // Whelp. ¯\_(ツ)_/¯
    None
}

#[derive(Clone, Debug)]
enum Input<'a> {
    File(&'a str, &'a Path, &'a str, u64),
    Expr(&'a str),
    Loop(&'a str, bool),
}

const DEFLATE_PATH_LEN_MAX: usize = 20;
const CONTENT_DIGEST_LEN_MAX: usize = 20;

impl<'a> Input<'a> {
    pub fn safe_name(&self) -> &str {
        use Input::*;

        match *self {
            File(name, _, _, _) => name,
            Expr(..) => "expr",
            Loop(..) => "loop",
        }
    }

    pub fn compute_id<'dep, DepIt>(&self, deps: DepIt) -> Result<OsString>
    where DepIt: IntoIterator<Item=(&'dep str, &'dep str)> {
        // use std::io::Write;
        use flate2::FlateWriteExt;
        use shaman::digest::Digest;
        use shaman::sha1::Sha1;
        use Input::*;

        // Hash all the common stuff now.
        let mut hasher = Sha1::new();
        for dep in deps {
            hasher.input_str("dep=");
            hasher.input_str(dep.0);
            hasher.input_str("=");
            hasher.input_str(dep.1);
            hasher.input_str(";");
        }

        match *self {
            File(name, path, content, _) => {
                let z_path = {
                    let buf: Vec<u8> = vec![];
                    let hex = Hexify(buf);
                    let mut z = hex.deflate_encode(flate2::Compression::Best);
                    try!(write!(z, "{}", path.display()));
                    let mut buf = try!(z.finish()).0;

                    buf.truncate(DEFLATE_PATH_LEN_MAX);
                    try!(String::from_utf8(buf)
                        .map_err(|_| "could not UTF-8 encode deflated path"))
                };

                hasher.input_str(&content);
                let mut digest = hasher.result_str();
                digest.truncate(CONTENT_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push("file-");
                id.push(name);
                id.push("-");
                id.push(if STUB_HASHES { "stub" } else { &*z_path });
                id.push("-");
                id.push(if STUB_HASHES { "stub" } else { &*digest });
                Ok(id)
            },
            Expr(content) => {
                hasher.input_str(&content);
                let mut digest = hasher.result_str();
                digest.truncate(CONTENT_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push("expr-");
                id.push(if STUB_HASHES { "stub" } else { &*digest });
                Ok(id)
            },
            Loop(content, count) => {
                hasher.input_str("count:");
                hasher.input_str(if count { "true;" } else { "false;" });

                hasher.input_str(&content);
                let mut digest = hasher.result_str();
                digest.truncate(CONTENT_DIGEST_LEN_MAX);

                let mut id = OsString::new();
                id.push("loop-");
                id.push(if STUB_HASHES { "stub" } else { &*digest });
                Ok(id)
            },
        }
    }
}

struct Hexify<W>(pub W) where W: Write;

impl<W> Write for Hexify<W>
where W: Write {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match buf.into_iter().next() {
            Some(b) => {
                try!(write!(self.0, "{:x}", b));
                Ok(1)
            },
            None => Ok(0)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub fn sha1_str(s: &str) -> String {
    use shaman::digest::Digest;
    use shaman::sha1::Sha1;
    let mut hasher = Sha1::new();
    hasher.input_str(&s);
    hasher.result_str()
}

mod win32 {
    #![allow(non_snake_case)]

    extern crate ole32;
    extern crate shell32;
    extern crate winapi;
    extern crate uuid;

    use std::ffi::OsString;
    use std::fmt;
    use std::mem;
    use std::os::windows::ffi::OsStringExt;
    pub use self::uuid::FOLDERID_LocalAppData;
    pub use error::MainError;

    pub type WinResult<T> = Result<T, WinError>;

    pub struct WinError(winapi::HRESULT);

    impl fmt::Display for WinError {
        fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
            write!(fmt, "HRESULT({})", self.0)
        }
    }

    impl From<WinError> for MainError {
        fn from(v: WinError) -> MainError {
            v.to_string().into()
        }
    }

    pub fn SHGetKnownFolderPath(rfid: &winapi::KNOWNFOLDERID, dwFlags: winapi::DWORD, hToken: winapi::HANDLE) -> WinResult<OsString> {
        use self::winapi::PWSTR;
        let mut psz_path: PWSTR = unsafe { mem::uninitialized() };
        let hresult = unsafe {
            shell32::SHGetKnownFolderPath(
                rfid,
                dwFlags,
                hToken,
                mem::transmute(&mut psz_path as &mut PWSTR as *mut PWSTR)
            )
        };

        if hresult == winapi::S_OK {
            let r = unsafe { pwstr_to_os_string(psz_path) };
            unsafe { ole32::CoTaskMemFree(psz_path as *mut _) };
            Ok(r)
        } else {
            Err(WinError(hresult))
        }
    }

    unsafe fn pwstr_to_os_string(ptr: winapi::PWSTR) -> OsString {
        OsStringExt::from_wide(::std::slice::from_raw_parts(ptr, pwstr_len(ptr)))
    }

    unsafe fn pwstr_len(mut ptr: winapi::PWSTR) -> usize {
        let mut len = 0;
        while *ptr != 0 {
            len += 1;
            ptr = ptr.offset(1);
        }
        len
    }
}
