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
    {
        let sb = sandbox.lock().map_err(|e| e.to_string())?;
        sb.check_access(root, 'r')?;
    }
    if Path::new(root).is_file() {
        {
            let sb = sandbox.lock().map_err(|e| e.to_string())?;
            sb.check_extensions(root, 'r')?;
        }
        return Ok(vec![root.to_string()]);
    }
    if !Path::new(root).is_dir() {
        return Err(format!("root '{root}' is not a file or directory"));
    }
    let mut out = Vec::new();
    collect_files(Path::new(root), sandbox, &mut out)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::FsModule;
    use serde_json::{Map, Value};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn setup() -> (PathBuf, FsModule) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("search-mod-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "ALPHA = 1\nBETA = 2\n").unwrap();
        fs::write(root.join("b.txt"), "hello world\n").unwrap();
        let sb = Sandbox::from_fs(
            &[format!("w:{}", root.display())],
            &[],
            &[".txt".into()],
            &[],
            &[],
        );
        (root, FsModule::new(Arc::new(Mutex::new(sb))))
    }

    fn setup_deprecated() -> (PathBuf, PathBuf, Arc<Mutex<Sandbox>>, FsModule) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("search-dep-{nanos}"));
        let target = root.join("target");
        fs::create_dir_all(target.join("nested")).unwrap();
        fs::write(target.join("hidden.txt"), "DEPRECATED_HIT\n").unwrap();
        fs::write(target.join("nested").join("deep.txt"), "DEPRECATED_HIT\n").unwrap();
        let banned = root.join("secret.log");
        fs::write(&banned, "DEPRECATED_HIT\n").unwrap();
        fs::write(root.join("ok.txt"), "VISIBLE_HIT\n").unwrap();
        let sb = Arc::new(Mutex::new(Sandbox::from_fs(
            &[format!("w:{}", root.display())],
            &[],
            &[],
            &[target.display().to_string()],
            &[banned.display().to_string()],
        )));
        (root, target, sb.clone(), FsModule::new(sb))
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

    #[test]
    fn search_skips_deprecated_and_does_not_count_them() {
        let (root, target, sb, m) = setup_deprecated();
        let files = iter_files_for_search(&sb, &root.to_string_lossy()).unwrap();
        assert!(
            files
                .iter()
                .all(|f| !f.contains("hidden") && !f.contains("deep") && !f.contains("secret")),
            "{files:?}"
        );
        assert!(files.iter().any(|f| f.ends_with("ok.txt")), "{files:?}");

        let found = m
            .call(
                "find_str",
                &args(&[
                    ("root", &root.to_string_lossy()),
                    ("string", "DEPRECATED_HIT"),
                ]),
            )
            .unwrap()
            .output;
        assert_eq!(found, "");

        let visible = m
            .call(
                "find_str",
                &args(&[("root", &root.to_string_lossy()), ("string", "VISIBLE_HIT")]),
            )
            .unwrap()
            .output;
        assert!(visible.contains("VISIBLE_HIT"), "{visible}");

        let by_name = m
            .call(
                "find_file_by_str",
                &args(&[("root", &root.to_string_lossy()), ("string", "hidden")]),
            )
            .unwrap()
            .output;
        assert_eq!(by_name, "");

        let by_re = m
            .call(
                "find_files_by_regex",
                &args(&[("root", &root.to_string_lossy()), ("regex", r"secret.*")]),
            )
            .unwrap()
            .output;
        assert_eq!(by_re, "");

        let re_content = m
            .call(
                "find_str_by_regex",
                &args(&[
                    ("root", &root.to_string_lossy()),
                    ("regex", "DEPRECATED_HIT"),
                ]),
            )
            .unwrap()
            .output;
        assert_eq!(re_content, "");

        let err = m
            .call(
                "find_str",
                &args(&[("root", &target.to_string_lossy()), ("string", "x")]),
            )
            .unwrap_err();
        assert!(err.contains("cannot find_str"), "{err}");
        assert!(err.contains("deprecated"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }
}
