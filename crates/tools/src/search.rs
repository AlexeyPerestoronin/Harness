//! Search tool set (`tools.search`): substring and regex scan under the sandbox.
//!
//! Progress summaries are written to stdout with in-place overwrite (not through
//! Logger), matching the Python `find_str` / `find_str_by_regex` format.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use regex::Regex;
use serde_json::{json, Map, Value};

use crate::sandbox::Sandbox;
use crate::{arg_str, CallResult, ToolDescriptor};

/// Search tools bound to the shared sandbox.
pub struct SearchModule {
    sandbox: Arc<Mutex<Sandbox>>,
}

impl SearchModule {
    /// Wrap an existing sandbox.
    pub fn new(sandbox: Arc<Mutex<Sandbox>>) -> Self {
        Self { sandbox }
    }

    /// JSON Schema descriptors for the search tools.
    pub fn descriptors() -> Vec<ToolDescriptor> {
        vec![
            desc("find_str", "Поиск строки в файлах начиная с корневого каталога.", json!({"type":"object","properties":{"root":{"type":"string"},"string":{"type":"string"}},"required":["root","string"]})),
            desc("find_str_by_regex", "Поиск строк по регулярному выражению начиная с корневого каталога.", json!({"type":"object","properties":{"root":{"type":"string"},"regex":{"type":"string"}},"required":["root","regex"]})),
        ]
    }

    /// Dispatch a named search tool.
    pub fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match name {
            "find_str" => Ok(CallResult::plain(self.find_str(
                &arg_str(args, "root")?,
                &arg_str(args, "string")?,
            )?)),
            "find_str_by_regex" => Ok(CallResult::plain(self.find_str_by_regex(
                &arg_str(args, "root")?,
                &arg_str(args, "regex")?,
            )?)),
            _ => Err(format!("tool '{name}' not available")),
        }
    }

    fn find_str(&self, root: &str, string: &str) -> Result<String, String> {
        let needle = string.to_string();
        self.scan_files(
            root,
            "find_str",
            &[("root", root), ("string", string)],
            Box::new(move |content| content.contains(&needle)),
        )
    }

    fn find_str_by_regex(&self, root: &str, regex: &str) -> Result<String, String> {
        let pattern = Regex::new(regex).map_err(|e| format!("cannot find_str_by_regex → {e}"))?;
        self.scan_files(
            root,
            "find_str_by_regex",
            &[("root", root), ("regex", regex)],
            Box::new(move |content| pattern.is_match(content)),
        )
    }

    fn scan_files(
        &self,
        root: &str,
        title: &str,
        fields: &[(&str, &str)],
        line_matches: Box<dyn Fn(&str) -> bool>,
    ) -> Result<String, String> {
        let mut matches = Vec::new();
        let mut files_observed = 0usize;
        let mut match_detected = 0usize;
        let mut prev_lines = 0usize;
        let mut active = String::new();
        (|| {
            for path in self.iter_files_for_search(root)? {
                files_observed += 1;
                active = path.clone();
                prev_lines = emit_search_progress(
                    title,
                    fields,
                    files_observed,
                    match_detected,
                    &active,
                    prev_lines,
                );
                let Ok(text) = fs::read_to_string(&path) else {
                    continue;
                };
                for line in text.lines() {
                    let content = line.trim_end_matches(['\r', '\n']);
                    if line_matches(content) {
                        match_detected += 1;
                        matches.push(format!("{path}:{content}"));
                    }
                }
                prev_lines = emit_search_progress(
                    title,
                    fields,
                    files_observed,
                    match_detected,
                    &active,
                    prev_lines,
                );
            }
            if files_observed == 0 {
                emit_search_progress(
                    title,
                    fields,
                    files_observed,
                    match_detected,
                    &active,
                    prev_lines,
                );
            }
            Ok::<_, String>(matches.join("\n"))
        })()
        .map_err(|e| format!("cannot {title} → {e}"))
    }

    fn iter_files_for_search(&self, root: &str) -> Result<Vec<String>, String> {
        {
            let sb = self.sandbox.lock().map_err(|e| e.to_string())?;
            sb.check_access(root, 'r')?;
        }
        if Path::new(root).is_file() {
            {
                let sb = self.sandbox.lock().map_err(|e| e.to_string())?;
                sb.check_extensions(root, 'r')?;
            }
            return Ok(vec![root.to_string()]);
        }
        if !Path::new(root).is_dir() {
            return Err(format!("root '{root}' is not a file or directory"));
        }
        let mut out = Vec::new();
        collect_files(Path::new(root), &self.sandbox, &mut out)?;
        Ok(out)
    }
}

fn collect_files(
    dir: &Path,
    sandbox: &Arc<Mutex<Sandbox>>,
    out: &mut Vec<String>,
) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for e in entries {
        let path = e.path();
        if path.is_dir() {
            dirs.push(path);
        } else {
            files.push(path);
        }
    }
    for f in files {
        let s = f.to_string_lossy().into_owned();
        let sb = sandbox.lock().map_err(|e| e.to_string())?;
        if sb.check_access(&s, 'r').is_ok() && sb.check_extensions(&s, 'r').is_ok() {
            out.push(s);
        }
    }
    for d in dirs {
        collect_files(&d, sandbox, out)?;
    }
    Ok(())
}

fn emit_search_progress(
    title: &str,
    fields: &[(&str, &str)],
    files_observed: usize,
    match_detected: usize,
    active: &str,
    prev_lines: usize,
) -> usize {
    let field_block = fields
        .iter()
        .map(|(n, v)| format!("  {n} = {v}"))
        .collect::<Vec<_>>()
        .join(",\n");
    let text = format!(
        "{title}(\n{field_block}\n) result:\n- files observed: {files_observed}\n- match detected: {match_detected}\n- active scanning from: {active}\n"
    );
    let mut stdout = std::io::stdout();
    if prev_lines > 0 {
        let _ = write!(stdout, "\x1b[{prev_lines}A\x1b[J");
    }
    let _ = write!(stdout, "{text}");
    let _ = stdout.flush();
    text.matches('\n').count()
}

fn desc(name: &str, description: &str, parameters: Value) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::Sandbox;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn setup() -> (PathBuf, SearchModule) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("search-mod-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "ALPHA = 1\nBETA = 2\n").unwrap();
        fs::write(root.join("b.txt"), "hello world\n").unwrap();
        let sb = Sandbox::from_fs(&[format!("w:{}", root.display())], &[], &[".txt".into()]);
        (root, SearchModule::new(Arc::new(Mutex::new(sb))))
    }

    fn args(pairs: &[(&str, &str)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), Value::String((*v).to_string())))
            .collect()
    }

    #[test]
    fn find_str_and_regex() {
        let (root, m) = setup();
        let found = m
            .call(
                "find_str",
                &args(&[("root", &root.to_string_lossy()), ("string", "BETA = 2")]),
            )
            .unwrap()
            .output;
        assert!(found.contains("BETA = 2"), "{found}");
        assert!(found.contains("a.txt"), "{found}");

        let re = m
            .call(
                "find_str_by_regex",
                &args(&[("root", &root.to_string_lossy()), ("regex", r"BETA = \d+")]),
            )
            .unwrap()
            .output;
        assert!(re.contains("BETA = 2"), "{re}");

        let none = m
            .call(
                "find_str",
                &args(&[
                    ("root", &root.to_string_lossy()),
                    ("string", "does-not-exist-xyz"),
                ]),
            )
            .unwrap()
            .output;
        assert_eq!(none, "");

        let err = m
            .call(
                "find_str_by_regex",
                &args(&[("root", &root.to_string_lossy()), ("regex", "[unclosed")]),
            )
            .unwrap_err();
        assert!(err.contains("cannot find_str_by_regex"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn access_denied_outside_sandbox() {
        let (root, m) = setup();
        let err = m
            .call(
                "find_str",
                &args(&[("root", "/no/such/outside"), ("string", "x")]),
            )
            .unwrap_err();
        assert!(err.contains("cannot find_str"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }
}
