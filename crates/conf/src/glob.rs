//! Translation of `tools.fs.deprecated-dir-patterns` glob patterns into regex.
//!
//! The supported syntax is intentionally strict: every pattern must be
//! `**/<component>`. Inside `<component>` the only wildcard is `*` (zero or
//! more characters except `/`). Every other character, including `?`, `[`, `]`
//! and regex metacharacters, is matched literally. Raw regex is not supported.

use crate::error::ConfError;

/// Translate a `deprecated-dir-patterns` entry into a regex source string.
///
/// `\` is normalized to `/`. On Windows the pattern is lowercased so that a
/// case-sensitive regex can still match paths case-insensitively.
pub fn regex_source(pattern: &str) -> Result<String, ConfError> {
    let pattern = pattern.replace('\\', "/");
    let pattern = if cfg!(windows) {
        pattern.to_lowercase()
    } else {
        pattern
    };

    let rest = match pattern.strip_prefix("**/") {
        Some(rest) => rest,
        None => {
            if pattern == "**" {
                return Err(invalid_pattern(&pattern, "component must not be empty"));
            }
            let reason = match pattern.find('/') {
                Some(pos) => format!("unexpected '/' at position {pos}"),
                None => "pattern must start with '**/'".to_string(),
            };
            return Err(invalid_pattern(&pattern, reason));
        }
    };

    if rest.is_empty() {
        return Err(invalid_pattern(&pattern, "component must not be empty"));
    }

    let mut out = String::from("(?:^|/)");
    for (idx, ch) in rest.char_indices() {
        if ch == '/' {
            return Err(invalid_pattern(
                &pattern,
                format!("unexpected '/' at position {}", 3 + idx),
            ));
        }
        if rest[idx..].starts_with("**") {
            return Err(invalid_pattern(&pattern, "component must not contain '**'"));
        }
        if ch == '*' {
            out.push_str("[^/]*");
        } else if ch == '-' {
            out.push('-');
        } else {
            out.push_str(&regex::escape(&ch.to_string()));
        }
    }
    out.push_str("(?:/|$)");
    Ok(out)
}

fn invalid_pattern(pattern: &str, reason: impl Into<String>) -> ConfError {
    ConfError::new(format!(
        "invalid 'deprecated-dir-patterns' entry '{pattern}':\n  {}\n  expected glob pattern '**/<component>'\n  only '*' is a wildcard inside <component>; other characters are matched literally\n  regex is not supported",
        reason.into()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_patterns_translate_to_expected_regex() {
        assert_eq!(regex_source("**/log").unwrap(), "(?:^|/)log(?:/|$)");
        assert_eq!(regex_source("**/done").unwrap(), "(?:^|/)done(?:/|$)");
        assert_eq!(
            regex_source("**/note-*").unwrap(),
            "(?:^|/)note-[^/]*(?:/|$)"
        );
        assert_eq!(regex_source("**/log?").unwrap(), "(?:^|/)log\\?(?:/|$)");
    }

    #[test]
    fn invalid_patterns_report_position_and_help() {
        for pattern in ["log/**", "**/log/**", "foo/**/bar", "**", "**/log/"] {
            let err = regex_source(pattern).unwrap_err().to_string();
            assert!(err.contains(pattern), "missing pattern in error: {err}");
            assert!(
                err.contains("expected glob pattern '**/<component>'"),
                "{err}"
            );
            assert!(
                err.contains("only '*' is a wildcard inside <component>"),
                "{err}"
            );
            assert!(err.contains("regex is not supported"), "{err}");
        }
    }

    #[test]
    fn nested_star_star_is_rejected() {
        let err = regex_source("**/a**b").unwrap_err().to_string();
        assert!(err.contains("component must not contain '**'"), "{err}");
    }
}
