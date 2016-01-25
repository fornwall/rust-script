/*
Copyright ⓒ 2015 cargo-script contributors.

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

use self::regex::Regex;
use toml;

use consts;
use error::{Blame, Result};
use util::SubsliceOffset;
use Input;

lazy_static! {
    static ref RE_SHORT_MANIFEST: Regex = Regex::new(
        r"^(?i)\s*//\s*cargo-deps\s*:(.*?)(\r\n|\n)").unwrap();
    static ref RE_MARGIN: Regex = Regex::new(r"^\s*\*( |$)").unwrap();
    static ref RE_SPACE: Regex = Regex::new(r"^(\s+)").unwrap();
    static ref RE_NESTING: Regex = Regex::new(r"/\*|\*/").unwrap();
    static ref RE_COMMENT: Regex = Regex::new(r"^\s*//!").unwrap();
    static ref RE_HASHBANG: Regex = Regex::new(r"^#![^[].*?(\r\n|\n)").unwrap();
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
    let (part_mani, source, template, sub_prelude) = match *input {
        Input::File(_, _, content, _) => {
            assert_eq!(prelude_items.len(), 0);
            let content = strip_hashbang(content);
            let (manifest, source) = find_embedded_manifest(content)
                .unwrap_or((Manifest::Toml(""), content));

            (manifest, source, consts::FILE_TEMPLATE, false)
        },
        Input::Expr(content) => {
            (Manifest::Toml(""), content, consts::EXPR_TEMPLATE, true)
        },
        Input::Loop(content, count) => {
            let templ = if count { consts::LOOP_COUNT_TEMPLATE } else { consts::LOOP_TEMPLATE };
            (Manifest::Toml(""), content, templ, true)
        },
    };

    let source = template.replace("%b", source);

    /*
    We are doing it this way because we can guarantee that %p *always* appears before %b, *and* that we don't attempt this when we don't want to allow prelude substitution.

    The problem with doing it the other way around is that the user could specify a prelude item that contains `%b` (which would do *weird things*).

    Also, don't use `str::replace` because it replaces *all* occurrences, not just the first.
    */
    let source = match sub_prelude {
        false => source,
        true => {
            const PRELUDE_PAT: &'static str = "%p";
            let offset = source.find(PRELUDE_PAT).expect("template doesn't have %p");
            let mut new_source = String::new();
            new_source.push_str(&source[..offset]);
            for i in prelude_items {
                new_source.push_str(i);
                new_source.push_str("\n");
            }
            new_source.push_str(&source[offset + PRELUDE_PAT.len()..]);
            new_source
        }
    };

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

    assert_eq!(si!(f(
r#"
// Cargo-Deps: time="0.1.25", libc="0.2.5"
fn main() {}
"#
        )),
        r!(
r#"
[[bin]]
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
        Some((_, end)) => &s[end..],
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
            Toml(s) => Ok(try!(toml::Parser::new(s).parse()
                .ok_or("could not parse embedded manifest"))),
            TomlOwned(ref s) => Ok(try!(toml::Parser::new(s).parse()
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
        .or_else(|| find_code_block_manifest(s))
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
Locates a "prefix manifest" embedded in a Cargoified Rust Script file.
*/
fn find_prefix_manifest(content: &str) -> Option<(Manifest, &str)> {
    /*
    The trick with this is that we *will not* assume the input is correctly formed, or that we've been passed a file that even *has* an embedded manifest; *i.e.* we might have been run with a plain Rust source file.

    We look for something which indicates the end of the embedded manifest.  *Officially*, this is a line which contains nothing but whitespace and *at least* three hyphens.  In *truth*, we will also look for anything that looks like Rust code.

    Specifically, we check for a line starting with any of the strings in `SPLIT_MARKERS`.  This should *hopefully* cover every possible valid Rust program.

    Once we've done that, we just chop the script content up in the appropriate places.
    */
    let lines = content.lines();

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
            // subslices can only be None if me/ss are *not* substrings of content.
            (&content[..content.subslice_offset_stable(me).unwrap()],
                &content[content.subslice_offset_stable(ss).unwrap()..])
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
    let re = &*RE_SHORT_MANIFEST;
    if let Some(cap) = re.captures(s) {
        if let Some((a, b)) = cap.pos(1) {
            return Some((Manifest::DepList(&s[a..b]), &s[..]))
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
        Some(cap) => match cap.pos(1) {
            Some((a, _)) => a,
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
        fn code_block(&mut self, output: &mut Buffer, text: &Buffer, lang: &Buffer) {
            use std::ascii::AsciiExt;

            let lang = lang.to_str().unwrap();

            if !self.seen_manifest && lang.eq_ignore_ascii_case("cargo") {
                // Pass it through.
                info!("found code block manifest");
                output.pipe(text);
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

            for (end, marker) in nesting_re.find_iter(line).map(|(a,b)| (a, &line[a..b])) {
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
                    .and_then(|(b, e)| Some(&line[b..e])));

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
                    .map(|(_,n)| n));

            /*
            Make sure we have only leading spaces.

            If we see a tab, fall over.  I *would* expand them, but that gets into the question of how *many* spaces to expand them to, and *where* is the tab, because tabs are tab stops and not just N spaces.

            Eurgh.
            */
            try!(n_leading_spaces(line, leading_space.unwrap_or(0)));

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
                Some((_, end)) => &line[end..],
                None => break
            };

            // Detect and strip leading indentation.
            leading_space = leading_space
                .or_else(|| space_re.captures(content)
                    .and_then(|c| c.pos(1))
                    .map(|(_,n)| n));

            /*
            Make sure we have only leading spaces.

            If we see a tab, fall over.  I *would* expand them, but that gets into the question of how *many* spaces to expand them to, and *where* is the tab, because tabs are tab stops and not just N spaces.

            Eurgh.
            */
            try!(n_leading_spaces(content, leading_space.unwrap_or(0)));

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
