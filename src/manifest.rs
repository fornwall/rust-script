/*!
This module is concerned with how `rust-script` extracts the manfiest from a script file.
*/
use regex;

use self::regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use crate::consts;
use crate::error::{MainError, MainResult};
use crate::templates;
use crate::Input;
use log::{error, info};

/**
Splits input into a complete Cargo manifest and unadultered Rust source.

Unless we have prelude items to inject, in which case it will be *slightly* adulterated.
*/
#[allow(clippy::too_many_arguments)]
pub fn split_input(
    input: &Input,
    base_path: &Path,
    deps: &[(String, String)],
    prelude_items: &[String],
    package_path: impl AsRef<Path>,
    bin_name: &str,
    script_name: &str,
    toolchain: Option<String>,
) -> MainResult<(String, PathBuf, Option<String>)> {
    fn contains_main_method(source: &str) -> bool {
        let re_main: Regex =
            Regex::new(r#"(?m)^ *(pub )?(async )?(extern "C" )?fn main *\("#).unwrap();
        re_main.is_match(source)
    }

    let source_in_package = package_path.as_ref().join(script_name);
    let (part_mani, source_path, source, template, sub_prelude) = match input {
        Input::File(_, path, content, _) => {
            assert_eq!(prelude_items.len(), 0);
            let content = strip_shebang(content);
            let (manifest, source) =
                find_embedded_manifest(content).unwrap_or((Manifest::Toml(""), content));

            if contains_main_method(content) {
                (manifest, path.clone(), source.to_string(), None, false)
            } else {
                (
                    manifest,
                    source_in_package,
                    content.to_string(),
                    Some(consts::FILE_NO_MAIN_TEMPLATE),
                    false,
                )
            }
        }
        Input::Expr(content, _) => (
            Manifest::Toml(""),
            source_in_package,
            content.to_string(),
            Some(consts::EXPR_TEMPLATE),
            true,
        ),
        Input::Loop(content, count, _) => (
            Manifest::Toml(""),
            source_in_package,
            content.to_string(),
            Some(if *count {
                consts::LOOP_COUNT_TEMPLATE
            } else {
                consts::LOOP_TEMPLATE
            }),
            true,
        ),
    };

    let mut prelude_str;
    let mut subs = HashMap::with_capacity(2);

    subs.insert(consts::SCRIPT_BODY_SUB, &source[..]);

    if sub_prelude {
        prelude_str =
            String::with_capacity(prelude_items.iter().map(|i| i.len() + 1).sum::<usize>());
        for i in prelude_items {
            prelude_str.push_str(i);
            prelude_str.push('\n');
        }
        subs.insert(consts::SCRIPT_PRELUDE_SUB, &prelude_str[..]);
    }

    let source = template
        .map(|template| templates::expand(template, &subs))
        .transpose()?;
    let part_mani = part_mani.into_toml()?;
    info!("part_mani: {:?}", part_mani);
    info!("source: {:?}", source);

    let source_path_from_package = if template.is_some() {
        script_name
    } else {
        source_path
            .to_str()
            .ok_or_else(|| format!("Unable to stringify {source_path:?}"))?
    };

    // It's-a mergin' time!
    let def_mani = default_manifest(bin_name, source_path_from_package, toolchain);
    let dep_mani = deps_manifest(deps)?;

    let mani = merge_manifest(def_mani, part_mani)?;
    let mani = merge_manifest(mani, dep_mani)?;

    // Fix up relative paths.
    let mani = fix_manifest_paths(mani, base_path)?;

    let mani_str = format!("{}", mani);
    info!("manifest: {}", mani_str);

    Ok((mani_str, source_path, source))
}

#[cfg(test)]
pub const STRIP_SECTION: &str = r##"

[profile.release]
strip = true
"##;

#[test]
fn test_split_input() {
    let bin_name = "binary-name".to_string();
    let script_name = "main.rs".to_string();
    let toolchain = None;
    macro_rules! si {
        ($i:expr) => {
            split_input(
                &$i,
                &$i.base_path(),
                &[],
                &[],
                "/package",
                &bin_name,
                &script_name,
                toolchain.clone(),
            )
            .ok()
        };
    }

    let f = |c: &str| {
        let dummy_path: ::std::path::PathBuf = "/dummy/main.rs".into();
        Input::File(
            "n".to_string(),
            dummy_path.clone(),
            c.to_string(),
            dummy_path,
        )
    };

    macro_rules! r {
        ($m:expr, $p:expr, $r:expr) => {
            Some(($m.into(), $p.into(), $r.into()))
        };
    }

    assert_eq!(
        si!(f(r#"fn main() {}"#)),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "/dummy/main.rs"

[dependencies]

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0""#,
                STRIP_SECTION
            ),
            "/dummy/main.rs",
            None
        )
    );

    assert_eq!(
        si!(f(r#"#!/usr/bin/env rust-script
fn main() {}"#)),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "/dummy/main.rs"

[dependencies]

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0""#,
                STRIP_SECTION
            ),
            "/dummy/main.rs",
            None
        )
    );

    assert_eq!(
        si!(f(r#"#[thingy]
fn main() {}"#)),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "/dummy/main.rs"

[dependencies]

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0""#,
                STRIP_SECTION
            ),
            "/dummy/main.rs",
            None
        )
    );

    assert_eq!(
        split_input(
            &f(r#"fn main() {}"#),
            &f(r#"fn main() {}"#).base_path(),
            &[],
            &[],
            "",
            &bin_name,
            "main.rs",
            Some("stable".to_string())
        )
        .ok(),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "/dummy/main.rs"

[dependencies]

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0"

[package.metadata.rustscript]
toolchain = "stable""#,
                STRIP_SECTION
            ),
            "/dummy/main.rs",
            None
        )
    );

    // Ensure removed prefix manifests don't work.
    assert_eq!(
        si!(f(r#"
---
fn main() {}
"#)),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "/dummy/main.rs"

[dependencies]

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0""#,
                STRIP_SECTION
            ),
            "/dummy/main.rs",
            None
        )
    );

    assert_eq!(
        si!(f(r#"[dependencies]
time="0.1.25"
---
fn main() {}
"#)),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "/dummy/main.rs"

[dependencies]

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0""#,
                STRIP_SECTION
            ),
            "/dummy/main.rs",
            None
        )
    );

    assert_eq!(
        si!(f(r#"
// Cargo-Deps: time="0.1.25"
fn main() {}
"#)),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "/dummy/main.rs"

[dependencies]
time = "0.1.25"

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0""#,
                STRIP_SECTION
            ),
            "/dummy/main.rs",
            None
        )
    );

    assert_eq!(
        si!(f(r#"
// Cargo-Deps: time="0.1.25", libc="0.2.5"
fn main() {}
"#)),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "/dummy/main.rs"

[dependencies]
libc = "0.2.5"
time = "0.1.25"

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0""#,
                STRIP_SECTION
            ),
            "/dummy/main.rs",
            None
        )
    );

    assert_eq!(
        si!(f(r#"
/*!
Here is a manifest:

```cargo
[dependencies]
time = "0.1.25"
```
*/
fn main() {}
"#)),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "/dummy/main.rs"

[dependencies]
time = "0.1.25"

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0""#,
                STRIP_SECTION
            ),
            "/dummy/main.rs",
            None
        )
    );

    assert_eq!(
        si!(f(r#"#!/usr/bin/env rust-script
println!("Hello")"#)),
        r!(
            format!(
                "{}{}",
                r#"[[bin]]
name = "binary-name"
path = "main.rs"

[dependencies]

[package]
authors = ["Anonymous"]
edition = "2021"
name = "binary-name"
version = "0.1.0""#,
                STRIP_SECTION
            ),
            "/package/main.rs",
            Some(
                r#"
fn main() -> Result<(), Box<dyn std::error::Error+Sync+Send>> {
    {println!("Hello")}
    Ok(())
}
"#
                .to_string()
            )
        )
    );
}

/**
Returns a slice of the input string with the leading shebang, if there is one, omitted.
*/
fn strip_shebang(s: &str) -> &str {
    let re_shebang: Regex = Regex::new(r"^#![^\[].*?(\r\n|\n)").unwrap();
    match re_shebang.find(s) {
        Some(m) => &s[m.end()..],
        None => s,
    }
}

/**
Represents the kind, and content of, an embedded manifest.
*/
#[derive(Debug, Eq, PartialEq)]
enum Manifest<'s> {
    /// The manifest is a valid TOML fragment.
    Toml(&'s str),
    /// The manifest is a valid TOML fragment (owned).
    // TODO: Change to Cow<'s, str>.
    TomlOwned(String),
    /// The manifest is a comma-delimited list of dependencies.
    DepList(&'s str),
}

impl<'s> Manifest<'s> {
    pub fn into_toml(self) -> MainResult<toml::value::Table> {
        use self::Manifest::*;
        match self {
            Toml(s) => toml::from_str(s),
            TomlOwned(ref s) => toml::from_str(s),
            DepList(s) => Manifest::dep_list_to_toml(s),
        }
        .map_err(|e| {
            MainError::Tag(
                "could not parse embedded manifest".into(),
                Box::new(MainError::Other(Box::new(e))),
            )
        })
    }

    fn dep_list_to_toml(s: &str) -> ::std::result::Result<toml::value::Table, toml::de::Error> {
        let mut r = String::new();
        r.push_str("[dependencies]\n");
        for dep in s.trim().split(',') {
            // If there's no version specified, add one.
            match dep.contains('=') {
                true => {
                    r.push_str(dep);
                    r.push('\n');
                }
                false => {
                    r.push_str(dep);
                    r.push_str("=\"*\"\n");
                }
            }
        }

        toml::from_str(&r)
    }
}

/**
Locates a manifest embedded in Rust source.

Returns `Some((manifest, source))` if it finds a manifest, `None` otherwise.
*/
fn find_embedded_manifest(s: &str) -> Option<(Manifest, &str)> {
    find_short_comment_manifest(s).or_else(|| find_code_block_manifest(s))
}

#[test]
fn test_find_embedded_manifest() {
    use self::Manifest::*;

    let fem = find_embedded_manifest;

    assert_eq!(fem("fn main() {}"), None);

    assert_eq!(
        fem("
fn main() {}
"),
        None
    );

    // Ensure removed prefix manifests don't work.
    assert_eq!(
        fem(r#"
---
fn main() {}
"#),
        None
    );

    assert_eq!(
        fem("[dependencies]
time = \"0.1.25\"
---
fn main() {}
"),
        None
    );

    assert_eq!(
        fem("[dependencies]
time = \"0.1.25\"

fn main() {}
"),
        None
    );

    // Make sure we aren't just grabbing the *last* line.
    assert_eq!(
        fem("[dependencies]
time = \"0.1.25\"

fn main() {
    println!(\"Hi!\");
}
"),
        None
    );

    assert_eq!(
        fem("// cargo-deps: time=\"0.1.25\"
fn main() {}
"),
        Some((
            DepList(" time=\"0.1.25\""),
            "// cargo-deps: time=\"0.1.25\"
fn main() {}
"
        ))
    );

    assert_eq!(
        fem("// cargo-deps: time=\"0.1.25\", libc=\"0.2.5\"
fn main() {}
"),
        Some((
            DepList(" time=\"0.1.25\", libc=\"0.2.5\""),
            "// cargo-deps: time=\"0.1.25\", libc=\"0.2.5\"
fn main() {}
"
        ))
    );

    assert_eq!(
        fem("
  // cargo-deps: time=\"0.1.25\"  \n\
fn main() {}
"),
        Some((
            DepList(" time=\"0.1.25\"  "),
            "
  // cargo-deps: time=\"0.1.25\"  \n\
fn main() {}
"
        ))
    );

    assert_eq!(
        fem("/* cargo-deps: time=\"0.1.25\" */
fn main() {}
"),
        None
    );

    assert_eq!(
        fem(r#"//! [dependencies]
//! time = "0.1.25"
fn main() {}
"#),
        None
    );

    assert_eq!(
        fem(r#"//! ```Cargo
//! [dependencies]
//! time = "0.1.25"
//! ```
fn main() {}
"#),
        Some((
            TomlOwned(
                r#"[dependencies]
time = "0.1.25"
"#
                .into()
            ),
            r#"//! ```Cargo
//! [dependencies]
//! time = "0.1.25"
//! ```
fn main() {}
"#
        ))
    );

    assert_eq!(
        fem(r#"/*!
[dependencies]
time = "0.1.25"
*/
fn main() {}
"#),
        None
    );

    assert_eq!(
        fem(r#"/*!
```Cargo
[dependencies]
time = "0.1.25"
```
*/
fn main() {}
"#),
        Some((
            TomlOwned(
                r#"[dependencies]
time = "0.1.25"
"#
                .into()
            ),
            r#"/*!
```Cargo
[dependencies]
time = "0.1.25"
```
*/
fn main() {}
"#
        ))
    );

    assert_eq!(
        fem(r#"/*!
 * [dependencies]
 * time = "0.1.25"
 */
fn main() {}
"#),
        None
    );

    assert_eq!(
        fem(r#"/*!
 * ```Cargo
 * [dependencies]
 * time = "0.1.25"
 * ```
 */
fn main() {}
"#),
        Some((
            TomlOwned(
                r#"[dependencies]
time = "0.1.25"
"#
                .into()
            ),
            r#"/*!
 * ```Cargo
 * [dependencies]
 * time = "0.1.25"
 * ```
 */
fn main() {}
"#
        ))
    );
}

/**
Locates a "short comment manifest" in Rust source.
*/
fn find_short_comment_manifest(s: &str) -> Option<(Manifest, &str)> {
    let re: Regex = Regex::new(r"^(?i)\s*//\s*cargo-deps\s*:(.*?)(\r\n|\n)").unwrap();
    /*
    This is pretty simple: the only valid syntax for this is for the first, non-blank line to contain a single-line comment whose first token is `cargo-deps:`.  That's it.
    */
    if let Some(cap) = re.captures(s) {
        if let Some(m) = cap.get(1) {
            return Some((Manifest::DepList(m.as_str()), s));
        }
    }
    None
}

/**
Locates a "code block manifest" in Rust source.
*/
fn find_code_block_manifest(s: &str) -> Option<(Manifest, &str)> {
    let re_crate_comment: Regex = {
        Regex::new(
            r"(?x)
                # We need to find the first `/*!` or `//!` that *isn't* preceeded by something that would make it apply to anything other than the crate itself.  Because we can't do this accurately, we'll just require that the doc comment is the *first* thing in the file (after the optional shebang, which should already have been stripped).
                ^\s*
                (/\*!|//(!|/))
            "
        ).unwrap()
    };
    /*
    This has to happen in a few steps.

    First, we will look for and slice out a contiguous, inner doc comment which must be *the very first thing* in the file.  `#[doc(...)]` attributes *are not supported*.  Multiple single-line comments cannot have any blank lines between them.

    Then, we need to strip off the actual comment markers from the content.  Including indentation removal, and taking out the (optional) leading line markers for block comments.  *sigh*

    Then, we need to take the contents of this doc comment and feed it to a Markdown parser.  We are looking for *the first* fenced code block with a language token of `cargo`.  This is extracted and pasted back together into the manifest.
    */
    let start = match re_crate_comment.captures(s) {
        Some(cap) => match cap.get(1) {
            Some(m) => m.start(),
            None => return None,
        },
        None => return None,
    };

    let comment = match extract_comment(&s[start..]) {
        Ok(s) => s,
        Err(err) => {
            error!("error slicing comment: {}", err);
            return None;
        }
    };

    scrape_markdown_manifest(&comment).map(|m| (Manifest::TomlOwned(m), s))
}

/**
Extracts the first `Cargo` fenced code block from a chunk of Markdown.
*/
fn scrape_markdown_manifest(content: &str) -> Option<String> {
    use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

    // To match librustdoc/html/markdown.rs, opts.
    let exts = Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES;

    let md = Parser::new_ext(content, exts);

    let mut found = false;
    let mut output = None;

    for item in md {
        match item {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref info)))
                if info.to_lowercase() == "cargo" && output.is_none() =>
            {
                found = true;
            }
            Event::Text(ref text) if found => {
                let s = output.get_or_insert(String::new());
                s.push_str(text);
            }
            Event::End(Tag::CodeBlock(_)) if found => {
                found = false;
            }
            _ => (),
        }
    }

    output
}

#[test]
fn test_scrape_markdown_manifest() {
    macro_rules! smm {
        ($c:expr) => {
            scrape_markdown_manifest($c)
        };
    }

    assert_eq!(
        smm!(
            r#"There is no manifest in this comment.
"#
        ),
        None
    );

    assert_eq!(
        smm!(
            r#"There is no manifest in this comment.

```
This is not a manifest.
```

```rust
println!("Nor is this.");
```

    Or this.
"#
        ),
        None
    );

    assert_eq!(
        smm!(
            r#"This is a manifest:

```cargo
dependencies = { time = "*" }
```
"#
        ),
        Some(
            r#"dependencies = { time = "*" }
"#
            .into()
        )
    );

    assert_eq!(
        smm!(
            r#"This is *not* a manifest:

```
He's lying, I'm *totally* a manifest!
```

This *is*:

```cargo
dependencies = { time = "*" }
```
"#
        ),
        Some(
            r#"dependencies = { time = "*" }
"#
            .into()
        )
    );

    assert_eq!(
        smm!(
            r#"This is a manifest:

```cargo
dependencies = { time = "*" }
```

So is this, but it doesn't count:

```cargo
dependencies = { explode = true }
```
"#
        ),
        Some(
            r#"dependencies = { time = "*" }
"#
            .into()
        )
    );
}

/**
Extracts the contents of a Rust doc comment.
*/
fn extract_comment(s: &str) -> MainResult<String> {
    use std::cmp::min;

    fn n_leading_spaces(s: &str, n: usize) -> MainResult<()> {
        if !s.chars().take(n).all(|c| c == ' ') {
            return Err(format!("leading {:?} chars aren't all spaces: {:?}", n, s).into());
        }
        Ok(())
    }

    fn extract_block(s: &str) -> MainResult<String> {
        /*
        On every line:

        - update nesting level and detect end-of-comment
        - if margin is None:
            - if there appears to be a margin, set margin.
        - strip off margin marker
        - update the leading space counter
        - strip leading space
        - append content
        */
        let mut r = String::new();

        let margin_re: Regex = Regex::new(r"^\s*\*( |$)").unwrap();
        let space_re: Regex = Regex::new(r"^(\s+)").unwrap();
        let nesting_re: Regex = Regex::new(r"/\*|\*/").unwrap();

        let mut leading_space = None;
        let mut margin = None;
        let mut depth: u32 = 1;

        for line in s.lines() {
            if depth == 0 {
                break;
            }

            // Update nesting and look for end-of-comment.
            let mut end_of_comment = None;

            for (end, marker) in nesting_re.find_iter(line).map(|m| (m.start(), m.as_str())) {
                match (marker, depth) {
                    ("/*", _) => depth += 1,
                    ("*/", 1) => {
                        end_of_comment = Some(end);
                        depth = 0;
                        break;
                    }
                    ("*/", _) => depth -= 1,
                    _ => panic!("got a comment marker other than /* or */"),
                }
            }

            let line = end_of_comment.map(|end| &line[..end]).unwrap_or(line);

            // Detect and strip margin.
            margin = margin.or_else(|| margin_re.find(line).map(|m| m.as_str()));

            let line = if let Some(margin) = margin {
                let end = line
                    .char_indices()
                    .take(margin.len())
                    .map(|(i, c)| i + c.len_utf8())
                    .last()
                    .unwrap_or(0);
                &line[end..]
            } else {
                line
            };

            // Detect and strip leading indentation.
            leading_space = leading_space.or_else(|| space_re.find(line).map(|m| m.end()));

            /*
            Make sure we have only leading spaces.

            If we see a tab, fall over.  I *would* expand them, but that gets into the question of how *many* spaces to expand them to, and *where* is the tab, because tabs are tab stops and not just N spaces.

            Eurgh.
            */
            n_leading_spaces(line, leading_space.unwrap_or(0))?;

            let strip_len = min(leading_space.unwrap_or(0), line.len());
            let line = &line[strip_len..];

            // Done.
            r.push_str(line);

            // `lines` removes newlines.  Ideally, it wouldn't do that, but hopefully this shouldn't cause any *real* problems.
            r.push('\n');
        }

        Ok(r)
    }

    fn extract_line(s: &str) -> MainResult<String> {
        let mut r = String::new();

        let comment_re = Regex::new(r"^\s*//(!|/)").unwrap();

        let space_re = Regex::new(r"^(\s+)").unwrap();

        let mut leading_space = None;

        for line in s.lines() {
            // Strip leading comment marker.
            let content = match comment_re.find(line) {
                Some(m) => &line[m.end()..],
                None => break,
            };

            // Detect and strip leading indentation.
            leading_space = leading_space.or_else(|| {
                space_re
                    .captures(content)
                    .and_then(|c| c.get(1))
                    .map(|m| m.end())
            });

            /*
            Make sure we have only leading spaces.

            If we see a tab, fall over.  I *would* expand them, but that gets into the question of how *many* spaces to expand them to, and *where* is the tab, because tabs are tab stops and not just N spaces.

            Eurgh.
            */
            n_leading_spaces(content, leading_space.unwrap_or(0))?;

            let strip_len = min(leading_space.unwrap_or(0), content.len());
            let content = &content[strip_len..];

            // Done.
            r.push_str(content);

            // `lines` removes newlines.  Ideally, it wouldn't do that, but hopefully this shouldn't cause any *real* problems.
            r.push('\n');
        }

        Ok(r)
    }

    if let Some(stripped) = s.strip_prefix("/*!") {
        extract_block(stripped)
    } else if s.starts_with("//!") || s.starts_with("///") {
        extract_line(s)
    } else {
        Err("no doc comment found".into())
    }
}

#[test]
fn test_extract_comment() {
    macro_rules! ec {
        ($s:expr) => {
            extract_comment($s).map_err(|e| e.to_string())
        };
    }

    assert_eq!(ec!(r#"fn main () {}"#), Err("no doc comment found".into()));

    assert_eq!(
        ec!(r#"/*!
Here is a manifest:

```cargo
[dependencies]
time = "*"
```
*/
fn main() {}
"#),
        Ok(r#"
Here is a manifest:

```cargo
[dependencies]
time = "*"
```

"#
        .into())
    );

    assert_eq!(
        ec!(r#"/*!
 * Here is a manifest:
 *
 * ```cargo
 * [dependencies]
 * time = "*"
 * ```
 */
fn main() {}
"#),
        Ok(r#"
Here is a manifest:

```cargo
[dependencies]
time = "*"
```

"#
        .into())
    );

    assert_eq!(
        ec!(r#"//! Here is a manifest:
//!
//! ```cargo
//! [dependencies]
//! time = "*"
//! ```
fn main() {}
"#),
        Ok(r#"Here is a manifest:

```cargo
[dependencies]
time = "*"
```
"#
        .into())
    );
}

/**
Generates a default Cargo manifest for the given input.
*/
fn default_manifest(
    bin_name: &str,
    bin_source_path: &str,
    toolchain: Option<String>,
) -> toml::value::Table {
    let mut package_map = toml::map::Map::new();
    package_map.insert(
        "name".to_string(),
        toml::value::Value::String(bin_name.to_owned()),
    );
    package_map.insert(
        "version".to_string(),
        toml::value::Value::String("0.1.0".to_string()),
    );
    package_map.insert(
        "authors".to_string(),
        toml::value::Value::Array(vec![toml::value::Value::String("Anonymous".to_string())]),
    );
    package_map.insert(
        "edition".to_string(),
        toml::value::Value::String("2021".to_string()),
    );
    if let Some(toolchain) = toolchain {
        let mut metadata = toml::map::Map::new();
        let mut rustscript_metadata = toml::map::Map::new();
        rustscript_metadata.insert(
            "toolchain".to_string(),
            toml::value::Value::String(toolchain),
        );
        metadata.insert(
            "rustscript".to_string(),
            toml::value::Value::Table(rustscript_metadata),
        );
        package_map.insert("metadata".to_string(), toml::value::Value::Table(metadata));
    }

    let mut release_map = toml::map::Map::new();
    release_map.insert("strip".to_string(), toml::value::Value::Boolean(true));

    let mut profile_map = toml::map::Map::new();
    profile_map.insert(
        "release".to_string(),
        toml::value::Value::Table(release_map),
    );

    let mut bin_map = toml::map::Map::new();
    bin_map.insert(
        "name".to_string(),
        toml::value::Value::String(bin_name.to_string()),
    );

    bin_map.insert(
        "path".to_string(),
        toml::value::Value::String(bin_source_path.to_string()),
    );

    let mut mani_map = toml::map::Map::new();
    mani_map.insert(
        "bin".to_string(),
        toml::value::Value::Array(vec![toml::value::Value::Table(bin_map)]),
    );
    mani_map.insert(
        "package".to_string(),
        toml::value::Value::Table(package_map),
    );
    mani_map.insert(
        "profile".to_string(),
        toml::value::Value::Table(profile_map),
    );

    mani_map
}

/**
Generates a partial Cargo manifest containing the specified dependencies.
*/
fn deps_manifest(deps: &[(String, String)]) -> MainResult<toml::value::Table> {
    let mut mani_str = String::new();
    mani_str.push_str("[dependencies]\n");

    for (name, ver) in deps {
        mani_str.push_str(name);
        mani_str.push('=');

        // We only want to quote the version if it *isn't* a table.
        let quotes = match ver.starts_with('{') {
            true => "",
            false => "\"",
        };
        mani_str.push_str(quotes);
        mani_str.push_str(ver);
        mani_str.push_str(quotes);
        mani_str.push('\n');
    }

    toml::from_str(&mani_str).map_err(|e| {
        MainError::Tag(
            "could not parse dependency manifest".into(),
            Box::new(MainError::Other(Box::new(e))),
        )
    })
}

/**
Given two Cargo manifests, merges the second *into* the first.

Note that the "merge" in this case is relatively simple: only *top-level* tables are actually merged; everything else is just outright replaced.
*/
fn merge_manifest(
    mut into_t: toml::value::Table,
    from_t: toml::value::Table,
) -> MainResult<toml::value::Table> {
    for (k, v) in from_t {
        match v {
            toml::Value::Table(from_t) => {
                // Merge.
                match into_t.entry(k) {
                    toml::map::Entry::Vacant(e) => {
                        e.insert(toml::Value::Table(from_t));
                    }
                    toml::map::Entry::Occupied(e) => {
                        let into_t = as_table_mut(e.into_mut()).ok_or(
                            "cannot merge manifests: cannot merge \
                                table and non-table values",
                        )?;
                        into_t.extend(from_t);
                    }
                }
            }
            v => {
                // Just replace.
                into_t.insert(k, v);
            }
        }
    }

    return Ok(into_t);

    fn as_table_mut(t: &mut toml::Value) -> Option<&mut toml::value::Table> {
        match t {
            toml::Value::Table(t) => Some(t),
            _ => None,
        }
    }
}

/**
Given a Cargo manifest, attempts to rewrite relative file paths to absolute ones, allowing the manifest to be relocated.
*/
fn fix_manifest_paths(mani: toml::value::Table, base: &Path) -> MainResult<toml::value::Table> {
    // Values that need to be rewritten:
    let paths: &[&[&str]] = &[
        &["build-dependencies", "*", "path"],
        &["dependencies", "*", "path"],
        &["dev-dependencies", "*", "path"],
        &["package", "build"],
        &["target", "*", "dependencies", "*", "path"],
    ];

    let mut mani = toml::Value::Table(mani);

    for path in paths {
        iterate_toml_mut_path(&mut mani, path, &mut |v| {
            if let toml::Value::String(s) = v {
                if Path::new(s).is_relative() {
                    let p = base.join(&*s);
                    if let Some(p) = p.to_str() {
                        *s = p.into()
                    }
                }
            }
            Ok(())
        })?
    }

    match mani {
        toml::Value::Table(mani) => Ok(mani),
        _ => unreachable!(),
    }
}

/**
Iterates over the specified TOML values via a path specification.
*/
fn iterate_toml_mut_path<F>(
    base: &mut toml::Value,
    path: &[&str],
    on_each: &mut F,
) -> MainResult<()>
where
    F: FnMut(&mut toml::Value) -> MainResult<()>,
{
    if path.is_empty() {
        return on_each(base);
    }

    let cur = path[0];
    let tail = &path[1..];

    if cur == "*" {
        if let toml::Value::Table(tab) = base {
            for (_, v) in tab {
                iterate_toml_mut_path(v, tail, on_each)?;
            }
        }
    } else if let toml::Value::Table(tab) = base {
        if let Some(v) = tab.get_mut(cur) {
            iterate_toml_mut_path(v, tail, on_each)?;
        }
    }

    Ok(())
}
