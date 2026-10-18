//! Textual `${vars:...}` substitution for json5 conf files.
//!
//! Runs **before** `json5::from_str` so that a file may contain placeholders
//! that are not valid json5 until they are replaced. Each file has its own
//! top-level `vars` object; values from `harness.json5` are not visible in a
//! model file and vice versa.
//!
//! The `vars` section is left in the text (loaders ignore unknown keys).
//! Nested `${vars:...}` inside `vars` is an error. Comments are not scanned.
//!
//! Line numbers in subsequent json5 parse errors refer to the text **after**
//! substitution.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::error::ConfError;
use crate::str::keys;

/// Replace `${vars:<name>}` placeholders in `text`.
///
/// `source` is the file name used in error and warning messages.
pub(crate) fn substitute(text: &str, source: &str) -> Result<String, ConfError> {
    let prefix = subst_prefix();
    let vars_span = find_vars_section(text, source)?;
    if vars_span.is_none() && !text.contains(&prefix) {
        return Ok(text.to_string());
    }

    let defs = match vars_span {
        Some(span) => parse_vars(text, source, span, &prefix)?,
        None => HashMap::new(),
    };

    let mut used = HashSet::new();
    let out = apply(text, source, &defs, vars_span, &prefix, &mut used)?;

    for (name, def) in &defs {
        if !used.contains(name) {
            eprintln!(
                "{source}:{}:{}: warning → variable '{name}' is defined but not used",
                def.line, def.col
            );
        }
    }
    Ok(out)
}

#[derive(Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
    line: usize,
    col: usize,
}

struct VarDef {
    value: Value,
    line: usize,
    col: usize,
}

fn subst_prefix() -> String {
    format!("${{{}:", keys::VARS)
}

fn loc_error(source: &str, line: usize, col: usize, message: &str) -> ConfError {
    ConfError::new(format!("{source}:{line}:{col}: error → {message}"))
}

fn parse_vars(
    text: &str,
    source: &str,
    span: Span,
    prefix: &str,
) -> Result<HashMap<String, VarDef>, ConfError> {
    reject_nested(text, source, &span, prefix)?;
    let key_pos = collect_key_positions(text, &span);
    let vars_text = &text[span.start..span.end];
    let parsed: Value = json5::from_str(vars_text)
        .map_err(|_| loc_error(source, span.line, span.col, &not_object_msg()))?;
    let obj = parsed
        .as_object()
        .ok_or_else(|| loc_error(source, span.line, span.col, &not_object_msg()))?;

    let mut defs = HashMap::new();
    for (name, value) in obj {
        let (line, col) = key_pos
            .get(name)
            .copied()
            .unwrap_or((span.line, span.col));
        if !is_valid_var_name(name) {
            return Err(loc_error(
                source,
                line,
                col,
                &format!("invalid variable name '{name}'"),
            ));
        }
        defs.insert(
            name.clone(),
            VarDef {
                value: value.clone(),
                line,
                col,
            },
        );
    }
    Ok(defs)
}

fn not_object_msg() -> String {
    format!("{} is not an json5-object", keys::VARS)
}

fn reject_nested(text: &str, source: &str, span: &Span, prefix: &str) -> Result<(), ConfError> {
    let mut cur = Cursor::at(text, span.start, span.line, span.col);
    let mut state = ScanState::Code;
    while cur.i < span.end {
        if !state.in_comment() && match_subst(cur.rest(), prefix).is_some() {
            return Err(loc_error(
                source,
                cur.line,
                cur.col,
                "nested variables are not supported",
            ));
        }
        step_state(&mut cur, &mut state);
    }
    Ok(())
}

fn collect_key_positions(text: &str, span: &Span) -> HashMap<String, (usize, usize)> {
    let mut cur = Cursor::at(text, span.start, span.line, span.col);
    let mut out = HashMap::new();
    cur.skip_ws_comments();
    if cur.peek() != Some('{') {
        return out;
    }
    cur.bump();
    loop {
        cur.skip_ws_comments();
        match cur.peek() {
            None | Some('}') => break,
            Some(',') => {
                cur.bump();
                continue;
            }
            _ => {}
        }
        let line = cur.line;
        let col = cur.col;
        let Some(key) = cur.read_key() else {
            break;
        };
        cur.skip_ws_comments();
        if cur.peek() != Some(':') {
            break;
        }
        cur.bump();
        out.insert(key, (line, col));
        if !cur.skip_value() {
            break;
        }
    }
    out
}

fn find_vars_section(text: &str, source: &str) -> Result<Option<Span>, ConfError> {
    let mut cur = Cursor::new(text);
    cur.skip_ws_comments();
    if cur.peek() != Some('{') {
        return Ok(None);
    }
    cur.bump();
    let mut found = None;
    loop {
        cur.skip_ws_comments();
        match cur.peek() {
            None => return Ok(found),
            Some('}') => return Ok(found),
            Some(',') => {
                cur.bump();
                continue;
            }
            _ => {}
        }
        let Some(key) = cur.read_key() else {
            return Ok(found);
        };
        cur.skip_ws_comments();
        if cur.peek() != Some(':') {
            return Ok(found);
        }
        cur.bump();
        cur.skip_ws_comments();
        if key == keys::VARS {
            let start = cur.i;
            let line = cur.line;
            let col = cur.col;
            if cur.peek() != Some('{') {
                return Err(loc_error(source, line, col, &not_object_msg()));
            }
            if !cur.skip_value() {
                return Err(loc_error(source, line, col, &not_object_msg()));
            }
            found = Some(Span {
                start,
                end: cur.i,
                line,
                col,
            });
        } else if !cur.skip_value() {
            return Ok(found);
        }
    }
}

fn apply(
    text: &str,
    source: &str,
    defs: &HashMap<String, VarDef>,
    vars_span: Option<Span>,
    prefix: &str,
    used: &mut HashSet<String>,
) -> Result<String, ConfError> {
    let mut cur = Cursor::new(text);
    let mut state = ScanState::Code;
    let mut out = String::with_capacity(text.len());
    while !cur.eof() {
        if let Some(span) = &vars_span {
            if cur.i == span.start {
                out.push_str(&text[span.start..span.end]);
                cur.advance_bytes(span.end - cur.i);
                continue;
            }
        }
        if !state.in_comment() {
            if let Some(name) = match_subst(cur.rest(), prefix) {
                let def = defs.get(name).ok_or_else(|| {
                    loc_error(
                        source,
                        cur.line,
                        cur.col,
                        &format!("variable '{name}' is being used but not defined"),
                    )
                })?;
                used.insert(name.to_string());
                let ctx = state.context().expect("substitutions are not scanned in comments");
                out.push_str(&render(&def.value, ctx, source, cur.line, cur.col)?);
                let skip = prefix.len() + name.len() + 1;
                cur.advance_bytes(skip);
                match &mut state {
                    ScanState::Double { escaped } | ScanState::Single { escaped } => {
                        *escaped = false;
                    }
                    _ => {}
                }
                continue;
            }
        }
        let from = cur.i;
        step_state(&mut cur, &mut state);
        out.push_str(&text[from..cur.i]);
    }
    Ok(out)
}

#[derive(Clone, Copy)]
enum Context {
    Code,
    DoubleString,
    SingleString,
}

fn render(
    value: &Value,
    ctx: Context,
    source: &str,
    line: usize,
    col: usize,
) -> Result<String, ConfError> {
    match ctx {
        Context::Code => Ok(value.to_string()),
        Context::DoubleString | Context::SingleString => match value {
            Value::Array(_) | Value::Object(_) => Err(loc_error(
                source,
                line,
                col,
                "only scalar values can be embedded into a string",
            )),
            Value::String(s) => {
                let quote = match ctx {
                    Context::SingleString => '\'',
                    _ => '"',
                };
                Ok(escape_for_string(s, quote))
            }
            Value::Number(n) => Ok(n.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Null => Ok("null".to_string()),
        },
    }
}

fn escape_for_string(s: &str, quote: char) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{code:04x}", code = c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn match_subst<'a>(rest: &'a str, prefix: &str) -> Option<&'a str> {
    if !rest.starts_with(prefix) {
        return None;
    }
    let after = &rest[prefix.len()..];
    let name_len = after.bytes().take_while(|&b| is_var_name_byte(b)).count();
    if name_len == 0 {
        return None;
    }
    if after.as_bytes().get(name_len) != Some(&b'}') {
        return None;
    }
    Some(&after[..name_len])
}

fn is_var_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'
}

fn is_valid_var_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_var_name_byte)
}

#[derive(Clone, Copy)]
enum ScanState {
    Code,
    Double { escaped: bool },
    Single { escaped: bool },
    LineComment,
    BlockComment { star: bool },
}

impl ScanState {
    fn in_comment(self) -> bool {
        matches!(
            self,
            ScanState::LineComment | ScanState::BlockComment { .. }
        )
    }

    fn context(self) -> Option<Context> {
        match self {
            ScanState::Code => Some(Context::Code),
            ScanState::Double { .. } => Some(Context::DoubleString),
            ScanState::Single { .. } => Some(Context::SingleString),
            ScanState::LineComment | ScanState::BlockComment { .. } => None,
        }
    }
}

fn step_state(cur: &mut Cursor<'_>, state: &mut ScanState) {
    let Some(ch) = cur.peek() else {
        return;
    };
    match *state {
        ScanState::Code => {
            if ch == '"' {
                *state = ScanState::Double { escaped: false };
                cur.bump();
            } else if ch == '\'' {
                *state = ScanState::Single { escaped: false };
                cur.bump();
            } else if ch == '/' && cur.peek_nth(1) == Some('/') {
                *state = ScanState::LineComment;
                cur.bump();
                cur.bump();
            } else if ch == '/' && cur.peek_nth(1) == Some('*') {
                *state = ScanState::BlockComment { star: false };
                cur.bump();
                cur.bump();
            } else {
                cur.bump();
            }
        }
        ScanState::Double { escaped } => {
            if escaped {
                *state = ScanState::Double { escaped: false };
            } else if ch == '\\' {
                *state = ScanState::Double { escaped: true };
            } else if ch == '"' {
                *state = ScanState::Code;
            }
            cur.bump();
        }
        ScanState::Single { escaped } => {
            if escaped {
                *state = ScanState::Single { escaped: false };
            } else if ch == '\\' {
                *state = ScanState::Single { escaped: true };
            } else if ch == '\'' {
                *state = ScanState::Code;
            }
            cur.bump();
        }
        ScanState::LineComment => {
            if ch == '\n' {
                *state = ScanState::Code;
            }
            cur.bump();
        }
        ScanState::BlockComment { star } => {
            if star && ch == '/' {
                *state = ScanState::Code;
            } else {
                *state = ScanState::BlockComment { star: ch == '*' };
            }
            cur.bump();
        }
    }
}

struct Cursor<'a> {
    text: &'a str,
    i: usize,
    line: usize,
    col: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            i: 0,
            line: 1,
            col: 1,
        }
    }

    fn at(text: &'a str, i: usize, line: usize, col: usize) -> Self {
        Self {
            text,
            i,
            line,
            col,
        }
    }

    fn eof(&self) -> bool {
        self.i >= self.text.len()
    }

    fn rest(&self) -> &'a str {
        &self.text[self.i..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_nth(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.i += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn advance_bytes(&mut self, n: usize) {
        let end = self.i + n;
        while self.i < end && !self.eof() {
            self.bump();
        }
    }

    fn skip_ws_comments(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_nth(1) == Some('/') => {
                    self.bump();
                    self.bump();
                    while let Some(c) = self.peek() {
                        self.bump();
                        if c == '\n' {
                            break;
                        }
                    }
                }
                Some('/') if self.peek_nth(1) == Some('*') => {
                    self.bump();
                    self.bump();
                    let mut star = false;
                    while let Some(c) = self.peek() {
                        self.bump();
                        if star && c == '/' {
                            break;
                        }
                        star = c == '*';
                    }
                }
                _ => return,
            }
        }
    }

    fn read_key(&mut self) -> Option<String> {
        match self.peek()? {
            '"' => self.read_quoted_key('"'),
            '\'' => self.read_quoted_key('\''),
            c if is_ident_start(c) => Some(self.read_ident()),
            _ => None,
        }
    }

    fn read_ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if s.is_empty() {
                if !is_ident_start(c) {
                    break;
                }
            } else if !is_ident_continue(c) {
                break;
            }
            s.push(c);
            self.bump();
        }
        s
    }

    fn read_quoted_key(&mut self, quote: char) -> Option<String> {
        self.bump();
        let mut s = String::new();
        let mut escaped = false;
        loop {
            let c = self.bump()?;
            if escaped {
                s.push(match c {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    other => other,
                });
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == quote {
                return Some(s);
            }
            s.push(c);
        }
    }

    fn skip_string(&mut self, quote: char) -> bool {
        if self.peek() != Some(quote) {
            return false;
        }
        self.bump();
        let mut escaped = false;
        loop {
            let Some(c) = self.bump() else {
                return false;
            };
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == quote {
                return true;
            }
        }
    }

    fn skip_value(&mut self) -> bool {
        self.skip_ws_comments();
        match self.peek() {
            Some('{') => self.skip_balanced('{', '}'),
            Some('[') => self.skip_balanced('[', ']'),
            Some('"') => self.skip_string('"'),
            Some('\'') => self.skip_string('\''),
            Some(_) => self.skip_atom(),
            None => false,
        }
    }

    fn skip_balanced(&mut self, open: char, close: char) -> bool {
        if self.peek() != Some(open) {
            return false;
        }
        self.bump();
        let mut depth = 1;
        let mut state = ScanState::Code;
        while depth > 0 {
            let Some(ch) = self.peek() else {
                return false;
            };
            match state {
                ScanState::Code => {
                    if ch == open {
                        depth += 1;
                        self.bump();
                    } else if ch == close {
                        depth -= 1;
                        self.bump();
                    } else {
                        step_state(self, &mut state);
                    }
                }
                _ => step_state(self, &mut state),
            }
        }
        true
    }

    fn skip_atom(&mut self) -> bool {
        if self.peek().is_none() {
            return false;
        }
        if self.rest().starts_with("${") {
            while let Some(c) = self.bump() {
                if c == '}' {
                    return true;
                }
            }
            return false;
        }
        loop {
            match self.peek() {
                None => return true,
                Some(c) if c.is_whitespace() => return true,
                Some(',') | Some('}') | Some(']') => return true,
                Some('/') if matches!(self.peek_nth(1), Some('/') | Some('*')) => return true,
                Some(_) => {
                    self.bump();
                }
            }
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_file_line_col(err: &str, file: &str) {
        assert!(err.contains(file), "{err}");
        let rest = err
            .split(file)
            .nth(1)
            .unwrap_or("")
            .strip_prefix(':')
            .unwrap_or("");
        let mut parts = rest.split(':');
        let line = parts.next().unwrap_or("");
        let col = parts.next().unwrap_or("");
        assert!(
            !line.is_empty() && line.chars().all(|c| c.is_ascii_digit()),
            "{err}"
        );
        assert!(
            !col.is_empty() && col.chars().all(|c| c.is_ascii_digit()),
            "{err}"
        );
    }

    fn parsed(text: &str) -> Value {
        let out = substitute(text, "t.json5").unwrap();
        json5::from_str(&out).unwrap()
    }

    #[test]
    fn example_from_spec() {
        let text = r#"{
            "vars": {
                "test-var-1": "some substitution",
                "test-var-2": 3.14, // number
            },
            "key-1": "${vars:test-var-1} will be placed here",
            "key-2": ${vars:test-var-2}
        }"#;
        let v = parsed(text);
        assert_eq!(v["key-1"], "some substitution will be placed here");
        assert!((v["key-2"].as_f64().unwrap() - 3.14).abs() < 1e-10);
    }

    #[test]
    fn table_string_inside_string() {
        let v = parsed(
            r#"{
            "vars": { "s": "hello" },
            "k": "X ${vars:s} Y"
        }"#,
        );
        assert_eq!(v["k"], "X hello Y");
    }

    #[test]
    fn table_scalars_inside_string() {
        let v = parsed(
            r#"{
            "vars": { "n": 3.14, "b": true, "z": null },
            "k": "${vars:n} ${vars:b} ${vars:z}"
        }"#,
        );
        assert_eq!(v["k"], "3.14 true null");
    }

    #[test]
    fn table_array_inside_string_is_error() {
        let err = substitute(
            r#"{ "vars": { "a": [1, 2] }, "k": "x ${vars:a} y" }"#,
            "arr.json5",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("only scalar values can be embedded into a string"), "{err}");
        assert_file_line_col(&err, "arr.json5");
    }

    #[test]
    fn table_object_inside_string_is_error() {
        let err = substitute(
            r#"{ "vars": { "o": { "a": 1 } }, "k": "x ${vars:o} y" }"#,
            "obj.json5",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("only scalar values can be embedded into a string"), "{err}");
        assert_file_line_col(&err, "obj.json5");
    }

    #[test]
    fn table_string_outside_string() {
        let v = parsed(
            r#"{
            "vars": { "s": "hello" },
            "k": ${vars:s}
        }"#,
        );
        assert_eq!(v["k"], "hello");
    }

    #[test]
    fn table_literals_outside_string() {
        let v = parsed(
            r#"{
            "vars": {
                "n": 3.14,
                "b": false,
                "z": null,
                "a": [1, true],
                "o": { "z": null }
            },
            "n": ${vars:n},
            "b": ${vars:b},
            "z": ${vars:z},
            "a": ${vars:a},
            "o": ${vars:o}
        }"#,
        );
        assert!((v["n"].as_f64().unwrap() - 3.14).abs() < 1e-10);
        assert_eq!(v["b"], false);
        assert_eq!(v["z"], Value::Null);
        assert_eq!(v["a"], serde_json::json!([1, true]));
        assert_eq!(v["o"], serde_json::json!({"z": null}));
    }

    #[test]
    fn escapes_inside_double_quotes() {
        let v = parsed(
            r#"{
            "vars": { "s": "say \"hi\" \\ \nnext" },
            "k": "${vars:s}"
        }"#,
        );
        assert_eq!(v["k"], "say \"hi\" \\ \nnext");
    }

    #[test]
    fn escapes_inside_single_quotes() {
        let v = parsed(
            r#"{
            "vars": { "s": "a'b\"c" },
            'k': '${vars:s}'
        }"#,
        );
        assert_eq!(v["k"], "a'b\"c");
    }

    #[test]
    fn var_defined_after_use() {
        let v = parsed(
            r#"{
            "k": ${vars:x},
            "vars": { "x": 7 }
        }"#,
        );
        assert_eq!(v["k"], 7);
    }

    #[test]
    fn comments_are_ignored() {
        let text = r#"{
            "vars": { "a": 1 },
            // ${vars:unknown}
            /* ${vars:a} ${vars:missing} */
            "k": 1
        }"#;
        let v = parsed(text);
        assert_eq!(v["k"], 1);
        let out = substitute(text, "t.json5").unwrap();
        assert!(out.contains("${vars:unknown}"));
        assert!(out.contains("${vars:missing}"));
    }

    #[test]
    fn foreign_placeholders_left_intact() {
        let text = r#"{ "k": "${HOME}", "e": "${env:X}" }"#;
        let out = substitute(text, "t.json5").unwrap();
        assert_eq!(out, text);
        let v: Value = json5::from_str(&out).unwrap();
        assert_eq!(v["k"], "${HOME}");
        assert_eq!(v["e"], "${env:X}");
    }

    #[test]
    fn unknown_variable_error() {
        let err = substitute(r#"{ "k": ${vars:nope} }"#, "u.json5")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("variable 'nope' is being used but not defined"),
            "{err}"
        );
        assert_file_line_col(&err, "u.json5");
    }

    #[test]
    fn vars_not_object_error() {
        let err = substitute(r#"{ "vars": [1, 2], "k": 1 }"#, "v.json5")
            .unwrap_err()
            .to_string();
        assert!(err.contains("vars is not an json5-object"), "{err}");
        assert_file_line_col(&err, "v.json5");
    }

    #[test]
    fn nested_vars_error() {
        let err = substitute(
            r#"{ "vars": { "a": "${vars:b}", "b": 1 }, "k": ${vars:a} }"#,
            "n.json5",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("nested variables are not supported"), "{err}");
        assert_file_line_col(&err, "n.json5");
    }

    #[test]
    fn unused_var_is_not_an_error() {
        let v = parsed(r#"{ "vars": { "x": 1 }, "k": 2 }"#);
        assert_eq!(v["k"], 2);
    }

    #[test]
    fn file_without_vars_is_byte_identical() {
        let text = "{\n  \"cwd\": \"C:/w\",\n  \"k\": \"${HOME}\"\n}\n";
        let out = substitute(text, "t.json5").unwrap();
        assert_eq!(out.as_bytes(), text.as_bytes());
    }

    #[test]
    fn several_substitutions_in_one_string() {
        let v = parsed(
            r#"{
            "vars": { "a": "A", "b": "B" },
            "k": "${vars:a}-${vars:b}"
        }"#,
        );
        assert_eq!(v["k"], "A-B");
    }

    #[test]
    fn quoted_key_substitution() {
        let v = parsed(
            r#"{
            "vars": { "n": "cwd" },
            "${vars:n}": "x"
        }"#,
        );
        assert_eq!(v["cwd"], "x");
    }
}
