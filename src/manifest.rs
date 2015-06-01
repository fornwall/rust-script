/*!
This module is concerned with how `cargo-script` extracts the manfiest from a script file.
*/
extern crate regex;

use self::regex::Regex;
use toml;

use consts;
use error::{Blame, Result};
use Input;

/**
Splits input into a complete Cargo manifest and unadultered Rust source.
*/
pub fn split_input(input: &Input, deps: &[(String, String)]) -> Result<(String, String)> {
    let (part_mani, source, template) = match *input {
        Input::File(_, _, content, _) => {
            let content = strip_hashbang(content);
            let (manifest, source) = find_embedded_manifest(content)
                .unwrap_or((Manifest::Toml(""), content));

            (manifest, source, consts::FILE_TEMPLATE)
        },
        Input::Expr(content) => (Manifest::Toml(""), content, consts::EXPR_TEMPLATE),
        Input::Loop(content, count) => {
            let templ = if count { consts::LOOP_COUNT_TEMPLATE } else { consts::LOOP_TEMPLATE };
            (Manifest::Toml(""), content, templ)
        },
    };

    let source = template.replace("%%", source);

    info!("part_mani: {:?}", part_mani);
    info!("source: {:?}", source);

    let part_mani = try!(part_mani.into_toml());
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

#[test]
fn test_split_input() {
    macro_rules! si {
        ($i:expr) => (split_input(&$i, &[]).ok())
    }

    let dummy_path: ::std::path::PathBuf = "p".into();
    let dummy_path = &dummy_path;
    let f = |c| Input::File("n", &dummy_path, c, 0);

    macro_rules! r {
        ($m:expr, $r:expr) => (Some(($m.into(), $r.into())));
    }

    assert_eq!(si!(f(
r#"fn main() {}"#
        )),
        r!(
r#"
[[bin]]
name = "n"
path = "n.rs"

[dependencies]

[package]
authors = ["Anonymous"]
name = "n"
version = "0.1.0"
"#,
r#"fn main() {}"#
        )
    );

    assert_eq!(si!(f(
r#"
---
fn main() {}
"#
        )),
        r!(
r#"
[[bin]]
name = "n"
path = "n.rs"

[dependencies]

[package]
authors = ["Anonymous"]
name = "n"
version = "0.1.0"
"#,
r#"
fn main() {}
"#
        )
    );

    assert_eq!(si!(f(
r#"[dependencies]
time="0.1.25"
---
fn main() {}
"#
        )),
        r!(
r#"
[[bin]]
name = "n"
path = "n.rs"

[dependencies]
time = "0.1.25"

[package]
authors = ["Anonymous"]
name = "n"
version = "0.1.0"
"#,
r#"
fn main() {}
"#
        )
    );

    assert_eq!(si!(f(
r#"
// Cargo-Deps: time="0.1.25"
fn main() {}
"#
        )),
        r!(
r#"
[[bin]]
name = "n"
path = "n.rs"

[dependencies]
time = "0.1.25"

[package]
authors = ["Anonymous"]
name = "n"
version = "0.1.0"
"#,
r#"
// Cargo-Deps: time="0.1.25"
fn main() {}
"#
        )
    );
}

/**
Returns a slice of the input string with the leading hashbang, if there is one, omitted.
*/
fn strip_hashbang(s: &str) -> &str {
    use std::str::pattern::{Pattern, Searcher};
    use util::ToMultiPattern;

    if s.starts_with("#!") && !s.starts_with("#![") {
        let mut search = vec!["\r\n", "\n"].to_multi_pattern().into_searcher(s);
        match search.next_match() {
            Some((_, b)) => &s[b..],
            None => s
        }
    } else {
        s
    }
}

#[test]
fn test_strip_hashbang() {
    assert_eq!(strip_hashbang("\
#!/usr/bin/env cargo-script-run
and the rest
\
        "), "\
and the rest
\
        ");
    assert_eq!(strip_hashbang("\
#![thingy]
and the rest
\
        "), "\
#![thingy]
and the rest
\
        ");
}

/**
Represents the kind, and content of, an embedded manifest.
*/
#[derive(Debug, Eq, PartialEq)]
enum Manifest<'s> {
    /// The manifest is a valid TOML fragment.
    Toml(&'s str),
    /// The manifest is a comma-delimited list of dependencies.
    DepList(&'s str),
}

impl<'s> Manifest<'s> {
    pub fn into_toml(self) -> Result<toml::Table> {
        use self::Manifest::*;
        match self {
            Toml(s) => Ok(try!(toml::Parser::new(s).parse()
                .ok_or("could not parse embedded manifest"))),
            DepList(s) => Manifest::dep_list_to_toml(s),
        }
    }

    fn dep_list_to_toml(s: &str) -> Result<toml::Table> {
        let mut r = String::new();
        r.push_str("[dependencies]\n");
        for dep in s.trim().split(',') {
            // If there's no version specified, add one.
            match dep.contains('=') {
                true => {
                    r.push_str(dep);
                    r.push_str("\n");
                },
                false => {
                    r.push_str(dep);
                    r.push_str("=\"*\"\n");
                }
            }
        }

        Ok(try!(toml::Parser::new(&r).parse()
            .ok_or("could not parse embedded manifest")))
    }
}

/**
Locates a manifest embedded in Rust source.

Returns `Some((manifest, source))` if it finds a manifest, `None` otherwise.
*/
fn find_embedded_manifest(s: &str) -> Option<(Manifest, &str)> {
    find_short_comment_manifest(s)
        .or_else(|| find_prefix_manifest(s))
}

#[test]
fn test_find_embedded_manifest() {
    use self::Manifest::*;

    let fem = find_embedded_manifest;

    assert_eq!(fem("fn main() {}"), None);

    assert_eq!(fem(
"
fn main() {}
"),
    None);

    assert_eq!(fem(
r#"
---
fn main() {}
"#),
    Some((
Toml(r#"
"#),
r#"
fn main() {}
"#
    )));

    assert_eq!(fem(
"[dependencies]
time = \"0.1.25\"
---
fn main() {}
"),
    Some((
Toml("[dependencies]
time = \"0.1.25\"
"),
"
fn main() {}
"
    )));

    assert_eq!(fem(
"[dependencies]
time = \"0.1.25\"

fn main() {}
"),
    Some((
Toml("[dependencies]
time = \"0.1.25\"

"),
"fn main() {}
"
    )));

    // Make sure we aren't just grabbing the *last* line.
    assert_eq!(fem(
"[dependencies]
time = \"0.1.25\"

fn main() {
    println!(\"Hi!\");
}
"),
    Some((
Toml("[dependencies]
time = \"0.1.25\"

"),
"fn main() {
    println!(\"Hi!\");
}
"
    )));

    assert_eq!(fem(
"// cargo-deps: time=\"0.1.25\"
fn main() {}
"),
    Some((
DepList(" time=\"0.1.25\""),
"// cargo-deps: time=\"0.1.25\"
fn main() {}
"
    )));

    assert_eq!(fem(
"
  // cargo-deps: time=\"0.1.25\"  \n\
fn main() {}
"),
    Some((
DepList(" time=\"0.1.25\"  "),
"
  // cargo-deps: time=\"0.1.25\"  \n\
fn main() {}
"
    )));

    assert_eq!(fem(
"/* cargo-deps: time=\"0.1.25\" */
fn main() {}
"),
    None);
}

/**
Locates a "prefix manifest" embedded in a Cargoified Rust Script file.
*/
fn find_prefix_manifest(content: &str) -> Option<(Manifest, &str)> {
    /*
    The trick with this is that we *will not* assume the input is correctly formed, or that we've been passed a file that even *has* an embedded manifest; *i.e.* we might have been run with a plain Rust source file.

    We look for something which indicates the end of the embedded manifest.  *Officially*, this is a line which contains nothing but whitespace and *at least* three hyphens.  In *truth*, we will also look for anything that looks like Rust code.

    Specifically, we check for a line starting with any of the strings in `SPLIT_MARKERS`.  This should *hopefully* cover every possible valid Rust program.

    Once we've done that, we just chop the script content up in the appropriate places.
    */
    let lines = content.lines_any();

    let mut manifest_end = None;
    let mut source_start = None;
    let mut got_manifest_for_certain = false;

    'scan_lines: for line in lines {
        // Did we get a dash separator?
        let mut dashes = 0;
        if line.chars().all(|c| {
            if c == '-' { dashes += 1 }
            c.is_whitespace() || c == '-'
        }) && dashes >= 3 {
            info!("splitting because of dash divider in line {:?}", line);
            manifest_end = Some(&line[0..0]);
            source_start = Some(&line[line.len()..]);
            got_manifest_for_certain = true;
            break;
        }

        // Ok, it's-a guessin' time!  Yes, this is *evil*.
        const SPLIT_MARKERS: &'static [&'static str] = &[
            "//", "/*", "#![", "#[", "pub ",
            "extern ", "use ", "mod ", "type ",
            "struct ", "enum ", "fn ", "impl ", "impl<",
            "static ", "const ",
        ];

        let line_trimmed = line.trim_left();

        for marker in SPLIT_MARKERS {
            if line_trimmed.starts_with(marker) {
                info!("splitting because of marker '{:?}'", marker);
                manifest_end = Some(&line[0..]);
                source_start = Some(&line[0..]);
                break 'scan_lines;
            }
        }
    }

    let (manifest, source) = match (manifest_end, source_start) {
        (Some(me), Some(ss)) => {
            (&content[..content.subslice_offset(me)],
                &content[content.subslice_offset(ss)..])
        },
        _ => return None
    };

    // If the manifest doesn't contain anything but whitespace... then we can't really say we *found* a manifest...
    if !got_manifest_for_certain && manifest.chars().all(char::is_whitespace) {
        return None;
    }

    // Found one!
    Some((Manifest::Toml(manifest), source))
}

/**
Locates a "short comment manifest" in Rust source.
*/
fn find_short_comment_manifest(s: &str) -> Option<(Manifest, &str)> {
    /*
    This is pretty simple: the only valid syntax for this is for the first, non-blank line to contain a single-line comment whose first token is `cargo-deps:`.  That's it.
    */
    let re = Regex::new(r"^(?i)\s*//\s*cargo-deps\s*:(.*)(\r\n|\n)").unwrap();
    if let Some(cap) = re.captures(s) {
        if let Some((a, b)) = cap.pos(1) {
            return Some((Manifest::DepList(&s[a..b]), &s[..]))
        }
    }
    None
}

/**
Generates a default Cargo manifest for the given input.
*/
fn default_manifest(input: &Input) -> Result<toml::Table> {
    let mani_str = consts::DEFAULT_MANIFEST.replace("%n", input.safe_name());
    toml::Parser::new(&mani_str).parse()
        .ok_or("could not parse default manifest, somehow".into())
}

/**
Generates a partial Cargo manifest containing the specified dependencies.
*/
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

/**
Given two Cargo manifests, merges the second *into* the first.

Note that the "merge" in this case is relatively simple: only *top-level* tables are actually merged; everything else is just outright replaced.
*/
fn merge_manifest(mut into_t: toml::Table, from_t: toml::Table) -> Result<toml::Table> {
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
