//! Content and filename search used by the `fs` tool module.
//!
//! Progress summaries for content-search are written to stdout with in-place
//! overwrite (not through Logger), matching the previous `find_str` format.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use regex::Regex;

use super::Sandbox;

pub(super) fn find_str(
    sandbox: &Arc<Mutex<Sandbox>>,
    root: &str,
    string: &str,
) -> Result<String, String> {
    let needle = string.to_string();
    scan_files(
        sandbox,
        root,
        "find_str",
        &[("root", root), ("string", string)],
        Box::new(move |content| content.contains(&needle)),
    )
}

pub(super) fn find_str_by_regex(
    sandbox: &Arc<Mutex<Sandbox>>,
    root: &str,
    regex: &str,
) -> Result<String, String> {
    let pattern = Regex::new(regex).map_err(|e| format!("cannot find_str_by_regex → {e}"))?;
    scan_files(
        sandbox,
        root,
        "find_str_by_regex",
        &[("root", root), ("regex", regex)],
        Box::new(move |content| pattern.is_match(content)),
    )
}

pub(super) fn find_file_by_str(
    sandbox: &Arc<Mutex<Sandbox>>,
    root: &str,
    string: &str,
) -> Result<String, String> {
    find_files_by_name(sandbox, root, "find_file_by_str", |name| {
        name.contains(string)
    })
}

pub(super) fn find_files_by_regex(
    sandbox: &Arc<Mutex<Sandbox>>,
    root: &str,
    regex: &str,
) -> Result<String, String> {
    let pattern = Regex::new(regex).map_err(|e| format!("cannot find_files_by_regex → {e}"))?;
    find_files_by_name(sandbox, root, "find_files_by_regex", move |name| {
        pattern.is_match(name)
    })
}

fn find_files_by_name(
    sandbox: &Arc<Mutex<Sandbox>>,
    root: &str,
    title: &str,
    name_matches: impl Fn(&str) -> bool,
) -> Result<String, String> {
    (|| {
        let files = iter_files_for_search(sandbox, root)?;
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for path in files {
            let name = Path::new(&path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if name_matches(name) && seen.insert(path.clone()) {
                out.push(path);
            }
        }
        Ok(out.join("\n"))
    })()
    .map_err(|e: String| format!("cannot {title} → {e}"))
}

fn scan_files(
    sandbox: &Arc<Mutex<Sandbox>>,
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
        for path in iter_files_for_search(sandbox, root)? {
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

fn iter_files_for_search(sandbox: &Arc<Mutex<Sandbox>>, root: &str) -> Result<Vec<String>, String> {
    let abs_root = {
        let sb = sandbox.lock().map_err(|e| e.to_string())?;
        let abs_root = sb.resolve_in_root(root)?;
        sb.check_access(root, 'r')?;
        if abs_root.is_file() {
            sb.check_extensions(root, 'r')?;
            return Ok(vec![abs_root.to_string_lossy().into_owned()]);
        }
        if !abs_root.is_dir() {
            return Err(format!("root '{root}' is not a file or directory"));
        }
        abs_root
    };
    let mut out = Vec::new();
    collect_files(&abs_root, sandbox, &mut out)?;
    Ok(out)
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
        if sb.is_deprecated(&s) {
            continue;
        }
        if sb.check_access(&s, 'r').is_ok() && sb.check_extensions(&s, 'r').is_ok() {
            out.push(s);
        }
    }
    for d in dirs {
        let s = d.to_string_lossy().into_owned();
        {
            let sb = sandbox.lock().map_err(|e| e.to_string())?;
            if sb.is_deprecated(&s) {
                continue;
            }
        }
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
