/*!
This module contains code related to template support.
*/
use crate::consts;
use crate::error::{MainError, MainResult};
use lazy_static::lazy_static;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;

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
    if let Some(text) = builtin_template(name) {
        return Ok(text.into());
    }
    panic!("No such template: {name}");
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
