/*
Copyright ⓒ 2015-2017 cargo-script contributors.

Licensed under the MIT license (see LICENSE or <http://opensource.org
/licenses/MIT>) or the Apache License, Version 2.0 (see LICENSE of
<http://www.apache.org/licenses/LICENSE-2.0>), at your option. All
files in the project carrying such notice may not be copied, modified,
or distributed except according to those terms.
*/
/*!
This module is concerned with how `cargo-script` extracts the manfiest from a script file.
*/
extern crate hoedown;
extern crate regex;

use std::collections::HashMap;
use std::path::Path;
use self::regex::Regex;
use toml;

use consts;
use error::{Blame, Result};
use templates;
use Input;

lazy_static! {
    static ref RE_SHORT_MANIFEST: Regex = Regex::new(
        r"^(?i)\s*//\s*cargo-deps\s*:(.*?)(\r\n|\n)").unwrap();
    static ref RE_MARGIN: Regex = Regex::new(r"^\s*\*( |$)").unwrap();
    static ref RE_SPACE: Regex = Regex::new(r"^(\s+)").unwrap();
    static ref RE_NESTING: Regex = Regex::new(r"/\*|\*/").unwrap();
    static ref RE_COMMENT: Regex = Regex::new(r"^\s*//!").unwrap();
    static ref RE_HASHBANG: Regex = Regex::new(r"^#![^\[].*?(\r\n|\n)").unwrap();
    static ref RE_CRATE_COMMENT: Regex = {
        Regex::new(
            r"(?x)
                # We need to find the first `/*!` or `//!` that *isn't* preceeded by something that would make it apply to anything other than the crate itself.  Because we can't do this accurately, we'll just require that the doc comment is the *first* thing in the file (after the optional hashbang, which should already have been stripped).
                ^\s*
                (/\*!|//!)
            "
        ).unwrap()
    };
}

/**
Splits input into a complete Cargo manifest and unadultered Rust source.

Unless we have prelude items to inject, in which case it will be *slightly* adulterated.
*/
pub fn split_input(input: &Input, deps: &[(String, String)], prelude_items: &[String]) -> Result<(String, String)> {
    let template_buf;
    let (part_mani, source, template, sub_prelude) = match *input {
        Input::File(_, _, content, _) => {
            assert_eq!(prelude_items.len(), 0);
            let content = strip_hashbang(content);
            let (manifest, source) = find_embedded_manifest(content)
                .unwrap_or((Manifest::Toml(""), content));

            (manifest, source, templates::get_template("file")?, false)
        },
        Input::Expr("meaning-of-life", None) | Input::Expr("meaning_of_life", None) => {
            (Manifest::Toml(""), r#"
                println!("42");
                std::process::exit(42);
            "#, templates::get_template("expr")?, true)
        },
        Input::Expr(content, template) => {
            template_buf = templates::get_template(template.unwrap_or("expr"))?;
            let (manifest, template_src) = find_embedded_manifest(&template_buf)
                .unwrap_or((Manifest::Toml(""), &template_buf));
            (manifest, content, template_src.into(), true)
        },
        Input::Loop(content, count) => {
            let templ = if count { "loop-count" } else { "loop" };
            (Manifest::Toml(""), content, templates::get_template(templ)?, true)
        },
    };

    let mut prelude_str;
    let mut subs = HashMap::with_capacity(2);
    subs.insert(consts::SCRIPT_BODY_SUB, &source[..]);

    if sub_prelude {
        prelude_str = String::with_capacity(prelude_items
            .iter()
            .map(|i| i.len() + 1)
            .sum::<usize>());
        for i in prelude_items {
            prelude_str.push_str(i);
            prelude_str.push_str("\n");
        }
        subs.insert(consts::SCRIPT_PRELUDE_SUB, &prelude_str[..]);
    }

    let source = templates::expand(&template, &subs)?;

    info!("part_mani: {:?}", part_mani);
    info!("source: {:?}", source);

    let part_mani = part_mani.into_toml()?;
    info!("part_mani: {:?}", part_mani);

    // It's-a mergin' time!
    let def_mani = default_manifest(input)?;
    let dep_mani = deps_manifest(deps)?;

    let mani = merge_manifest(def_mani, part_mani)?;
    let mani = merge_manifest(mani, dep_mani)?;

    // Fix up relative paths.
    let mani = fix_manifest_paths(mani, &input.base_path())?;
    info!("mani: {:?}", mani);

    let mani_str = format!("{}", toml::Value::Table(mani));
    info!("mani_str: {}", mani_str);

    Ok((mani_str, source))
}

#[test]
fn test_split_input() {
    macro_rules! si {
        ($i:expr) => (split_input(&$i, &[], &[]).ok())
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
r#"[[bin]]
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

    // Ensure removed prefix manifests don't work.
    assert_eq!(si!(f(
r#"
---
fn main() {}
"#
        )),
        r!(
r#"[[bin]]
name = "n"
path = "n.rs"

[dependencies]

[package]
authors = ["Anonymous"]
name = "n"
version = "0.1.0"
"#,
r#"
---
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
r#"[[bin]]
name = "n"
path = "n.rs"

[dependencies]

[package]
authors = ["Anonymous"]
name = "n"
version = "0.1.0"
"#,
r#"[dependencies]
time="0.1.25"
---
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
r#"[[bin]]
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

    assert_eq!(si!(f(
r#"
// Cargo-Deps: time="0.1.25", libc="0.2.5"
fn main() {}
"#
        )),
        r!(
r#"[[bin]]
name = "n"
path = "n.rs"

[dependencies]
libc = "0.2.5"
time = "0.1.25"

[package]
authors = ["Anonymous"]
name = "n"
version = "0.1.0"
"#,
r#"
// Cargo-Deps: time="0.1.25", libc="0.2.5"
fn main() {}
"#
        )
    );

    assert_eq!(si!(f(
r#"
/*!
Here is a manifest:

```cargo
[dependencies]
time = "0.1.25"
```
*/
fn main() {}
"#
        )),
        r!(
r#"[[bin]]
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
/*!
Here is a manifest:

```cargo
[dependencies]
time = "0.1.25"
```
*/
fn main() {}
"#
        )
    );
}

/**
Returns a slice of the input string with the leading hashbang, if there is one, omitted.
*/
fn strip_hashbang(s: &str) -> &str {
    match RE_HASHBANG.find(s) {
        Some(m) => &s[m.end()..],
        None => s
    }
}

#[test]
fn test_strip_hashbang() {
    assert_eq!(strip_hashbang("\
#!/usr/bin/env run-cargo-script
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
    /// The manifest is a valid TOML fragment (owned).
    // TODO: Change to Cow<'s, str>.
    TomlOwned(String),
    /// The manifest is a comma-delimited list of dependencies.
    DepList(&'s str),
}

impl<'s> Manifest<'s> {
    pub fn into_toml(self) -> Result<toml::Table> {
        use self::Manifest::*;
        match self {
            Toml(s) => Ok(toml::Parser::new(s).parse()
                .ok_or("could not parse embedded manifest")?),
            TomlOwned(ref s) => Ok(toml::Parser::new(s).parse()
                .ok_or("could not parse embedded manifest")?),
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

        Ok(toml::Parser::new(&r).parse()
            .ok_or("could not parse embedded manifest")?)
    }
}

/**
Locates a manifest embedded in Rust source.

Returns `Some((manifest, source))` if it finds a manifest, `None` otherwise.
*/
fn find_embedded_manifest(s: &str) -> Option<(Manifest, &str)> {
    find_short_comment_manifest(s)
        .or_else(|| find_code_block_manifest(s))
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

    // Ensure removed prefix manifests don't work.
    assert_eq!(fem(
r#"
---
fn main() {}
"#),
    None);

    assert_eq!(fem(
"[dependencies]
time = \"0.1.25\"
---
fn main() {}
"),
    None);

    assert_eq!(fem(
"[dependencies]
time = \"0.1.25\"

fn main() {}
"),
    None);

    // Make sure we aren't just grabbing the *last* line.
    assert_eq!(fem(
"[dependencies]
time = \"0.1.25\"

fn main() {
    println!(\"Hi!\");
}
"),
    None);

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
"// cargo-deps: time=\"0.1.25\", libc=\"0.2.5\"
fn main() {}
"),
    Some((
DepList(" time=\"0.1.25\", libc=\"0.2.5\""),
"// cargo-deps: time=\"0.1.25\", libc=\"0.2.5\"
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

    assert_eq!(fem(
r#"//! [dependencies]
//! time = "0.1.25"
fn main() {}
"#),
    None);

    assert_eq!(fem(
r#"//! ```Cargo
//! [dependencies]
//! time = "0.1.25"
//! ```
fn main() {}
"#),
    Some((
TomlOwned(r#"[dependencies]
time = "0.1.25"
"#.into()),
r#"//! ```Cargo
//! [dependencies]
//! time = "0.1.25"
//! ```
fn main() {}
"#
    )));

    assert_eq!(fem(
r#"/*!
[dependencies]
time = "0.1.25"
*/
fn main() {}
"#),
    None);

    assert_eq!(fem(
r#"/*!
```Cargo
[dependencies]
time = "0.1.25"
```
*/
fn main() {}
"#),
    Some((
TomlOwned(r#"[dependencies]
time = "0.1.25"
"#.into()),
r#"/*!
```Cargo
[dependencies]
time = "0.1.25"
```
*/
fn main() {}
"#
    )));

    assert_eq!(fem(
r#"/*!
 * [dependencies]
 * time = "0.1.25"
 */
fn main() {}
"#),
    None);

    assert_eq!(fem(
r#"/*!
 * ```Cargo
 * [dependencies]
 * time = "0.1.25"
 * ```
 */
fn main() {}
"#),
    Some((
TomlOwned(r#"[dependencies]
time = "0.1.25"
"#.into()),
r#"/*!
 * ```Cargo
 * [dependencies]
 * time = "0.1.25"
 * ```
 */
fn main() {}
"#
    )));
}

/**
Locates a "short comment manifest" in Rust source.
*/
fn find_short_comment_manifest(s: &str) -> Option<(Manifest, &str)> {
    /*
    This is pretty simple: the only valid syntax for this is for the first, non-blank line to contain a single-line comment whose first token is `cargo-deps:`.  That's it.
    */
    let re = &*RE_SHORT_MANIFEST;
    if let Some(cap) = re.captures(s) {
        if let Some(m) = cap.get(1) {
            return Some((Manifest::DepList(m.as_str()), &s[..]))
        }
    }
    None
}

/**
Locates a "code block manifest" in Rust source.
*/
fn find_code_block_manifest(s: &str) -> Option<(Manifest, &str)> {
    /*
    This has to happen in a few steps.

    First, we will look for and slice out a contiguous, inner doc comment which must be *the very first thing* in the file.  `#[doc(...)]` attributes *are not supported*.  Multiple single-line comments cannot have any blank lines between them.

    Then, we need to strip off the actual comment markers from the content.  Including indentation removal, and taking out the (optional) leading line markers for block comments.  *sigh*

    Then, we need to take the contents of this doc comment and feed it to a Markdown parser.  We are looking for *the first* fenced code block with a language token of `cargo`.  This is extracted and pasted back together into the manifest.
    */
    let start = match RE_CRATE_COMMENT.captures(s) {
        Some(cap) => match cap.get(1) {
            Some(m) => m.start(),
            None => return None
        },
        None => return None
    };

    let comment = match extract_comment(&s[start..]) {
        Ok(s) => s,
        Err(err) => {
            error!("error slicing comment: {}", err);
            return None
        }
    };

    scrape_markdown_manifest(&comment)
        .unwrap_or(None)
        .map(|m| (Manifest::TomlOwned(m), s))
}

/**
Extracts the first `Cargo` fenced code block from a chunk of Markdown.
*/
fn scrape_markdown_manifest(content: &str) -> Result<Option<String>> {
    use self::hoedown::{Buffer, Markdown, Render};

    // To match librustdoc/html/markdown.rs, HOEDOWN_EXTENSIONS.
    let exts
        = hoedown::NO_INTRA_EMPHASIS
        | hoedown::TABLES
        | hoedown::FENCED_CODE
        | hoedown::AUTOLINK
        | hoedown::STRIKETHROUGH
        | hoedown::SUPERSCRIPT
        | hoedown::FOOTNOTES;

    let md = Markdown::new(&content).extensions(exts);

    struct ManifestScraper {
        seen_manifest: bool,
    }

    impl Render for ManifestScraper {
        fn code_block(&mut self, output: &mut Buffer, text: Option<&Buffer>, lang: Option<&Buffer>) {
            let lang = lang.map(|b| b.to_str().unwrap()).unwrap_or("");

            if !self.seen_manifest && lang.eq_ignore_ascii_case("cargo") {
                // Pass it through.
                info!("found code block manifest");
                if let Some(text) = text {
                    output.pipe(text);
                }
                self.seen_manifest = true;
            }
        }
    }

    let mut ms = ManifestScraper { seen_manifest: false };
    let mani_buf = ms.render(&md);

    if !ms.seen_manifest { return Ok(None) }
    mani_buf.to_str().map(|s| Some(s.into()))
        .map_err(|_| "error decoding manifest as UTF-8".into())
}

#[test]
fn test_scrape_markdown_manifest() {
    macro_rules! smm {
        ($c:expr) => (scrape_markdown_manifest($c).map_err(|e| e.to_string()));
    }

    assert_eq!(smm!(
r#"There is no manifest in this comment.
"#
        ),
Ok(None)
    );

    assert_eq!(smm!(
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
Ok(None)
    );

    assert_eq!(smm!(
r#"This is a manifest:

```cargo
dependencies = { time = "*" }
```
"#
        ),
Ok(Some(r#"dependencies = { time = "*" }
"#.into()))
    );

    assert_eq!(smm!(
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
Ok(Some(r#"dependencies = { time = "*" }
"#.into()))
    );

    assert_eq!(smm!(
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
Ok(Some(r#"dependencies = { time = "*" }
"#.into()))
    );
}

/**
Extracts the contents of a Rust doc comment.
*/
fn extract_comment(s: &str) -> Result<String> {
    use std::cmp::min;

    fn n_leading_spaces(s: &str, n: usize) -> Result<()> {
        if !s.chars().take(n).all(|c| c == ' ') {
            return Err(format!("leading {:?} chars aren't all spaces: {:?}", n, s).into())
        }
        Ok(())
    }

    fn extract_block(s: &str) -> Result<String> {
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

        let margin_re = &*RE_MARGIN;
        let space_re = &*RE_SPACE;
        let nesting_re = &*RE_NESTING;

        let mut leading_space = None;
        let mut margin = None;
        let mut depth: u32 = 1;

        for line in s.lines() {
            if depth == 0 { break }

            // Update nesting and look for end-of-comment.
            let mut end_of_comment = None;

            for (end, marker) in {
                nesting_re.find_iter(line)
                    .map(|m| (m.start(), m.as_str()))
            } {
                match (marker, depth) {
                    ("/*", _) => depth += 1,
                    ("*/", 1) => {
                        end_of_comment = Some(end);
                        depth = 0;
                        break;
                    },
                    ("*/", _) => depth -= 1,
                    _ => panic!("got a comment marker other than /* or */")
                }
            }

            let line = end_of_comment.map(|end| &line[..end]).unwrap_or(line);

            // Detect and strip margin.
            margin = margin
                .or_else(|| margin_re.find(line)
                    .and_then(|m| Some(m.as_str())));

            let line = if let Some(margin) = margin {
                let end = line.char_indices().take(margin.len())
                    .map(|(i,c)| i + c.len_utf8()).last().unwrap_or(0);
                &line[end..]
            } else {
                line
            };

            // Detect and strip leading indentation.
            leading_space = leading_space
                .or_else(|| space_re.find(line)
                    .map(|m| m.end()));

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
            r.push_str("\n");
        }

        Ok(r)
    }

    fn extract_line(s: &str) -> Result<String> {
        let mut r = String::new();

        let comment_re = &*RE_COMMENT;
        let space_re = &*RE_SPACE;

        let mut leading_space = None;

        for line in s.lines() {
            // Strip leading comment marker.
            let content = match comment_re.find(line) {
                Some(m) => &line[m.end()..],
                None => break
            };

            // Detect and strip leading indentation.
            leading_space = leading_space
                .or_else(|| space_re.captures(content)
                    .and_then(|c| c.get(1))
                    .map(|m| m.end()));

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
            r.push_str("\n");
        }

        Ok(r)
    }

    if s.starts_with("/*!") {
        extract_block(&s[3..])
    } else if s.starts_with("//!") {
        extract_line(s)
    } else {
        Err("no doc comment found".into())
    }
}

#[test]
fn test_extract_comment() {
    macro_rules! ec {
        ($s:expr) => (extract_comment($s).map_err(|e| e.to_string()))
    }

    assert_eq!(ec!(
r#"fn main () {}"#
        ),
Err("no doc comment found".into())
    );

    assert_eq!(ec!(
r#"/*!
Here is a manifest:

```cargo
[dependencies]
time = "*"
```
*/
fn main() {}
"#
        ),
Ok(r#"
Here is a manifest:

```cargo
[dependencies]
time = "*"
```

"#.into())
    );

    assert_eq!(ec!(
r#"/*!
 * Here is a manifest:
 *
 * ```cargo
 * [dependencies]
 * time = "*"
 * ```
 */
fn main() {}
"#
        ),
Ok(r#"
Here is a manifest:

```cargo
[dependencies]
time = "*"
```

"#.into())
    );

    assert_eq!(ec!(
r#"//! Here is a manifest:
//!
//! ```cargo
//! [dependencies]
//! time = "*"
//! ```
fn main() {}
"#
        ),
Ok(r#"Here is a manifest:

```cargo
[dependencies]
time = "*"
```
"#.into())
    );
}

/**
Generates a default Cargo manifest for the given input.
*/
fn default_manifest(input: &Input) -> Result<toml::Table> {
    let mani_str = {
        let pkg_name = input.package_name();
        let mut subs = HashMap::with_capacity(2);
        subs.insert(consts::MANI_NAME_SUB, &*pkg_name);
        subs.insert(consts::MANI_FILE_SUB, &input.safe_name()[..]);
        templates::expand(consts::DEFAULT_MANIFEST, &subs)?
    };
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
                        let into_t = as_table_mut(e.into_mut())
                            .ok_or((Blame::Human, "cannot merge manifests: cannot merge \
                                table and non-table values"))?;
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

/**
Given a Cargo manifest, attempts to rewrite relative file paths to absolute ones, allowing the manifest to be relocated.
*/
fn fix_manifest_paths(mani: toml::Table, base: &Path) -> Result<toml::Table> {
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
            match *v {
                toml::Value::String(ref mut s) => {
                    if Path::new(s).is_relative() {
                        let p = base.join(&*s);
                        match p.to_str() {
                            Some(p) => *s = p.into(),
                            None => {},
                        }
                    }
                },
                _ => {}
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
fn iterate_toml_mut_path<F>(base: &mut toml::Value, path: &[&str], on_each: &mut F) -> Result<()>
where F: FnMut(&mut toml::Value) -> Result<()> {
    if path.len() == 0 {
        return on_each(base);
    }

    let cur = path[0];
    let tail = &path[1..];

    if cur == "*" {
        match *base {
            toml::Value::Table(ref mut tab) => {
                for (_, v) in tab {
                    iterate_toml_mut_path(v, tail, on_each)?;
                }
            },
            _ => {},
        }
    } else {
        match *base {
            toml::Value::Table(ref mut tab) => {
                match tab.get_mut(cur) {
                    Some(v) => {
                        iterate_toml_mut_path(v, tail, on_each)?;
                    },
                    None => {},
                }
            },
            _ => {},
        }
    }

    Ok(())
}
