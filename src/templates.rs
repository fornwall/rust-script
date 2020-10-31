/*!
This module contains code related to template support.
*/
use crate::consts;
use crate::error::{MainError, MainResult};
use crate::platform;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;

lazy_static! {
    static ref RE_SUB: Regex = Regex::new(r#"#\{([A-Za-z_][A-Za-z0-9_]*)}"#).unwrap();
}

pub fn expand(src: &str, subs: &HashMap<&str, &str>) -> MainResult<String> {
    // The estimate of final size is the sum of the size of all the input.
    let sub_size = subs.iter().map(|(_, v)| v.len()).sum::<usize>();
    let est_size = src.len() + sub_size;

    let mut anchor = 0;
    let mut result = String::with_capacity(est_size);

    for m in RE_SUB.captures_iter(src) {
        // Concatenate the static bit just before the match.
        let (m_start, m_end) = {
            let m_0 = m.get(0).unwrap();
            (m_0.start(), m_0.end())
        };
        let prior_slice = anchor..m_start;
        anchor = m_end;
        result.push_str(&src[prior_slice]);

        // Concat the substitution.
        let sub_name = m.get(1).unwrap().as_str();
        match subs.get(sub_name) {
            Some(s) => result.push_str(s),
            None => {
                return Err(MainError::OtherOwned(format!(
                    "substitution `{}` in template is unknown",
                    sub_name
                )))
            }
        }
    }
    result.push_str(&src[anchor..]);
    Ok(result)
}

/**
Attempts to locate and load the contents of the specified template.
*/
pub fn get_template(name: &str) -> MainResult<Cow<'static, str>> {
    use std::io::Read;

    let base = platform::templates_dir()?;

    let file = fs::File::open(base.join(format!("{}.rs", name)))
        .map_err(MainError::from)
        .map_err(|e| {
            MainError::Tag(
                format!(
                    "template file `{}.rs` does not exist in {}",
                    name,
                    base.display()
                )
                .into(),
                Box::new(e),
            )
        });

    // If the template is one of the built-in ones, do fallback if it wasn't found on disk.
    if file.is_err() {
        if let Some(text) = builtin_template(name) {
            return Ok(text.into());
        }
    }

    let mut file = file?;

    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text.into())
}

fn builtin_template(name: &str) -> Option<&'static str> {
    Some(match name {
        "expr" => consts::EXPR_TEMPLATE,
        "file" => consts::FILE_TEMPLATE,
        "loop" => consts::LOOP_TEMPLATE,
        "loop-count" => consts::LOOP_COUNT_TEMPLATE,
        _ => return None,
    })
}

pub fn list() -> MainResult<()> {
    use std::ffi::OsStr;

    let t_path = platform::templates_dir()?;

    if !t_path.exists() {
        fs::create_dir_all(&t_path)?;
    }

    println!("Listing templates in {}", t_path.display());

    if !t_path.exists() {
        return Err(format!(
            "cannot list template directory `{}`: it does not exist",
            t_path.display()
        )
        .into());
    }

    if !t_path.is_dir() {
        return Err(format!(
            "cannot list template directory `{}`: it is not a directory",
            t_path.display()
        )
        .into());
    }

    for entry in fs::read_dir(&t_path)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let f_path = entry.path();
        if f_path.extension() != Some(OsStr::new("rs")) {
            continue;
        }
        if let Some(stem) = f_path.file_stem() {
            println!("{}", stem.to_string_lossy());
        }
    }
    Ok(())
}
