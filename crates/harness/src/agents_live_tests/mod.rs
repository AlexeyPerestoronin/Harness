//! Ignored live API tests that run a real agent loop on a tiny workspace file.
//!
//! Each vendor test loads [`test-settings.json5`](test-settings.json5) (a real
//! harness conf with placeholders), asks for the API key on stdin, and asks the
//! agent to edit [`test-file.txt`](test-file.txt) then revert that edit. Not
//! part of CI (`#[ignore]`).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use conf::load;

use crate::agent_loop::{self, AgentLoop};

const SETTINGS_TEMPLATE: &str = include_str!("test-settings.json5");
const TEST_FILE_REL: &str = "crates/harness/src/agents_live_tests/test-file.txt";
const MARKER: &str = "live-test-ok";

fn workspace_root() -> PathBuf {
    let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let canon = raw.canonicalize().unwrap_or(raw);
    strip_verbatim(canon)
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    }
    path
}

fn slash(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn write_test_conf(model_file: &str) -> PathBuf {
    let root = workspace_root();
    let model_path = root.join("agents").join(model_file);
    let test_file = root.join(TEST_FILE_REL);
    let conf_text = SETTINGS_TEMPLATE
        .replace("<cwd>", &slash(&root))
        .replace("<target>", &slash(&model_path))
        .replace("<test-file>", &slash(&test_file));
    let conf_path = std::env::temp_dir().join(format!(
        "harness-live-{}-settings.json5",
        model_file.replace('.', "-")
    ));
    fs::write(&conf_path, conf_text).expect("write live test conf");
    conf_path
}

fn read_api_key(vendor: &str) -> String {
    eprint!("API key for {vendor}: ");
    let _ = io::stderr().flush();
    let mut key = String::new();
    io::stdin()
        .read_line(&mut key)
        .expect("read API key from stdin");
    let key = key.trim().to_string();
    assert!(!key.is_empty(), "{vendor} API key must not be empty");
    key
}

struct RestoreCwd(PathBuf);

impl Drop for RestoreCwd {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

struct RestoreFile {
    path: PathBuf,
    original: Vec<u8>,
}

impl Drop for RestoreFile {
    fn drop(&mut self) {
        let _ = fs::write(&self.path, &self.original);
    }
}

fn run_live(model_file: &str) {
    let root = workspace_root();
    let prev_cwd = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(&root).expect("chdir workspace");
    let _cwd = RestoreCwd(prev_cwd);

    let test_file = root.join(TEST_FILE_REL);
    let original = fs::read(&test_file).expect("read test-file.txt");
    let _restore = RestoreFile {
        path: test_file.clone(),
        original: original.clone(),
    };

    let conf_path = write_test_conf(model_file);
    let mut settings = load(&conf_path).unwrap_or_else(|e| {
        panic!("cannot load live test conf '{}': {e}", conf_path.display())
    });
    settings.agent.api_key = read_api_key(&settings.agent.vendor);

    let log_dir = agent_loop::session_dir(&settings).expect("session dir");
    let log_file = log_dir.join("log.md");
    let dump_file = log_dir.join("dump.json");
    eprintln!("live test log: {}", log_file.display());

    let prompt = format!(
        "Внеси небольшое изменение в файл {TEST_FILE_REL}: допиши в конец одну строку `{MARKER}`. \
         Затем сразу верни файл к исходному содержимому (убери эту строку). \
         Другие файлы не трогай. Когда файл снова в исходном виде — остановись."
    );

    let mut loop_ = AgentLoop::new(&settings, &log_file, log_dir).expect("agent loop");
    loop_
        .start(Some(prompt), &dump_file, None)
        .unwrap_or_else(|e| panic!("live session failed: {e}"));

    let after = fs::read(&test_file).expect("read test-file.txt after session");
    assert_eq!(
        after, original,
        "{TEST_FILE_REL} must be reverted after the live session"
    );

    let log = fs::read_to_string(&log_file).unwrap_or_default();
    assert!(
        log.contains("write_file") || log.contains("apply_diff_patch"),
        "agent should have edited {TEST_FILE_REL}; log was:\n{log}"
    );
}

#[test]
fn test_settings_template_loads() {
    let conf_path = write_test_conf("grok-4.6.json5");
    let settings = load(&conf_path).expect("test-settings.json5 should load");
    assert_eq!(settings.agent.vendor, "SpaceXAI");
    assert_eq!(settings.agent.name, "Grok 4.6");
    assert!(!settings.agent.mode.is_safe());
    assert_eq!(settings.agent.token_limit, 32000);
    assert_eq!(settings.agent.iteration_limit, 10);
    let extra = &settings.tools.fs.as_ref().expect("fs").available_files;
    assert!(
        extra.iter().any(|e| e.contains("test-file.txt")),
        "{extra:?}"
    );
    assert!(settings.tools.git.is_some());
    assert!(settings.tools.user.is_none());
    assert!(settings.tools.terminal.is_none());
}

#[test]
#[ignore = "live API; cargo test -p harness -- --ignored --nocapture --test-threads=1"]
fn claude_sonnet_5_5() {
    run_live("claude-sonnet-5-5.json5");
}

#[test]
#[ignore = "live API; cargo test -p harness -- --ignored --nocapture --test-threads=1"]
fn grok_4_6() {
    run_live("grok-4.6.json5");
}

#[test]
#[ignore = "live API; cargo test -p harness -- --ignored --nocapture --test-threads=1"]
fn gemini_3_8_flash() {
    run_live("gemini-3.8-flash.json5");
}
