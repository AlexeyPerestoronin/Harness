//! Unit tests for CWD-first loading and `tools.fs.deprecated-dir-patterns`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::load::{load, load_server_config, resolve_in_cwd};
use crate::types::ServerConfigState;

const MODEL: &str = r#"{
    vendor: "SpaceXAI",
    name: "Grok 4.6",
    "api-key": "test-key",
    limits: { tokens: 128000, USD: 0.5, rpm: -1, tpm: -1, rpd: -1 },
    price: { "input-price": -1, "output-price": -1, "cached-price": -1 },
}"#;

const CONF: &str = r#"{
    agent: {
        config: "model.json5",
        mode: "safe",
        "token-limit": 128000,
        "iteration-limit": 50,
        "USD-limit": 0.5,
        "log-folder": ".log",
    },
    tools: {},
}"#;

fn temp_path(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("conf-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}

fn write_valid_conf(dir: &Path) {
    write(&dir.join("model.json5"), MODEL);
    write(&dir.join("harness.json5"), CONF);
}

#[test]
fn harness_json5_without_cwd_is_valid() {
    let dir = temp_path("without-cwd");
    write_valid_conf(&dir);
    let settings = load(&dir.join("harness.json5"), &dir).unwrap();
    assert!(settings.cwd.is_absolute());
    assert_eq!(settings.agent.name, "Grok 4.6");
    assert!(settings.source.get("cwd").is_none());
}

#[test]
fn harness_json5_with_cwd_is_rejected() {
    let dir = temp_path("with-cwd");
    write(&dir.join("model.json5"), MODEL);
    let conf = r#"{
        cwd: ".",
        agent: {
            config: "model.json5",
            mode: "safe",
            "token-limit": 128000,
            "iteration-limit": 50,
            "USD-limit": 0.5,
        },
        tools: {},
    }"#;
    write(&dir.join("harness.json5"), conf);
    let err = load(&dir.join("harness.json5"), &dir)
        .unwrap_err()
        .to_string();
    assert!(err.contains("--cwd"), "{err}");
    assert!(err.contains("cwd"), "{err}");
}

#[test]
fn unknown_root_key_is_rejected() {
    let dir = temp_path("unknown-root");
    write(&dir.join("model.json5"), MODEL);
    let conf = r#"{
        unknown: {},
        agent: {
            config: "model.json5",
            mode: "safe",
            "token-limit": 128000,
            "iteration-limit": 50,
            "USD-limit": 0.5,
        },
        tools: {},
    }"#;
    write(&dir.join("harness.json5"), conf);
    let err = load(&dir.join("harness.json5"), &dir)
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown root key 'unknown'"), "{err}");
}

#[test]
fn resolve_in_cwd_normalizes_and_rejects_outside_paths() {
    let base = temp_path("resolve");
    let cwd = base.join("work");
    fs::create_dir_all(&cwd).unwrap();

    let normalized = resolve_in_cwd("./a/../b", &cwd).unwrap();
    assert_eq!(normalized, cwd.join("b"));

    let err = resolve_in_cwd("../outside", &cwd).unwrap_err().to_string();
    assert!(err.contains("outside"), "{err}");

    let outside = base.join("outside");
    let err = resolve_in_cwd(outside.to_str().unwrap(), &cwd)
        .unwrap_err()
        .to_string();
    assert!(err.contains("outside"), "{err}");
}

#[test]
fn resolve_in_cwd_does_not_treat_prefix_as_inside() {
    let base = temp_path("prefix");
    let cwd = base.join("work");
    fs::create_dir_all(&cwd).unwrap();
    let outside = base.join("work2");
    let err = resolve_in_cwd(outside.to_str().unwrap(), &cwd).unwrap_err();
    assert!(err.to_string().contains("outside"), "{err}");
}

#[cfg(windows)]
#[test]
fn resolve_in_cwd_is_case_insensitive_on_windows() {
    let base = std::env::temp_dir();
    let cwd = base.join("conf-cwd-case-test");
    let raw = base.join("CONF-CWD-CASE-TEST").join("sub");
    let resolved = resolve_in_cwd(&raw.to_string_lossy(), &cwd).unwrap();
    assert!(resolved
        .to_string_lossy()
        .to_lowercase()
        .contains("conf-cwd-case-test"));
}

#[test]
fn load_server_config_checks_cwd_before_file_existence() {
    let base = temp_path("server-order");
    let cwd = base.join("cwd");
    fs::create_dir_all(&cwd).unwrap();

    let outside = base.join("missing.json5");
    let err = load_server_config(&outside, &cwd).unwrap_err();
    assert!(err.to_string().contains("outside"), "{err}");

    let inside = cwd.join("missing.json5");
    let state = load_server_config(&inside, &cwd).unwrap();
    assert!(matches!(state, ServerConfigState::Missing));
}

#[test]
fn load_fs_accepts_valid_deprecated_dir_patterns() {
    let dir = temp_path("fs-valid-patterns");
    write(&dir.join("model.json5"), MODEL);
    let conf = r#"{
        agent: {
            config: "model.json5",
            mode: "safe",
            "token-limit": 128000,
            "iteration-limit": 50,
            "USD-limit": 0.5,
        },
        tools: {
            fs: {
                "available-dirs": ["r:."],
                "deprecated-dirs": [],
                "deprecated-files": [],
                "deprecated-dir-patterns": ["**/log", "**/done", "**/note-*"],
            },
        },
    }"#;
    write(&dir.join("harness.json5"), conf);
    let settings = load(&dir.join("harness.json5"), &dir).unwrap();
    let fs_settings = settings.tools.fs.unwrap();
    assert_eq!(
        fs_settings.deprecated_dir_patterns,
        vec!["**/log", "**/done", "**/note-*"]
    );
}

#[test]
fn load_fs_rejects_invalid_deprecated_dir_patterns() {
    let dir = temp_path("fs-invalid-patterns");
    write(&dir.join("model.json5"), MODEL);
    let conf = r#"{
        agent: {
            config: "model.json5",
            mode: "safe",
            "token-limit": 128000,
            "iteration-limit": 50,
            "USD-limit": 0.5,
        },
        tools: {
            fs: {
                "available-dirs": ["r:."],
                "deprecated-dirs": [],
                "deprecated-files": [],
                "deprecated-dir-patterns": ["log/**"],
            },
        },
    }"#;
    write(&dir.join("harness.json5"), conf);
    let err = load(&dir.join("harness.json5"), &dir)
        .unwrap_err()
        .to_string();
    assert!(err.contains("deprecated-dir-patterns"), "{err}");
    assert!(err.contains("glob pattern"), "{err}");
}
