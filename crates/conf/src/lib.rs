//! Configuration crate for the native harness.
//!
//! This is the only crate allowed to open `harness.json5` and nested model
//! files. Callers receive typed [`Settings`] and never parse json5 themselves.
//! All json5 keys and defaults live in [`str::keys`].

pub mod str;

mod error;
mod load;
mod template;
mod types;

pub use error::ConfError;
pub use load::load;
pub use template::{template_text, write_new_conf};
pub use types::{
    AgentKind, AgentSettings, FsSettings, GitSettings, MetaSettings, Mode, SearchSettings,
    Settings, TerminalSettings, ToolsSettings, UserSettings,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::str::keys;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("harness-conf-{label}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &std::path::Path, body: &str) {
        fs::write(path, body).unwrap();
    }

    fn model_body(api_key: &str) -> String {
        format!(
            r#"{{
            "{vendor}": "SpaceXAI",
            "{name}": "Grok 4.6",
            "{api_key_k}": "{api_key}",
            "{token}": 500000,
            "{iter}": 50,
            "{usd}": -1
        }}"#,
            vendor = keys::VENDOR,
            name = keys::NAME,
            api_key_k = keys::API_KEY,
            api_key = api_key,
            token = keys::TOKEN_LIMIT,
            iter = keys::ITERATION_LIMIT,
            usd = keys::USD_LIMIT,
        )
    }

    fn harness_body(model_file: &str, tools: &str) -> String {
        format!(
            r#"{{
            "{cwd}": "C:/workspace",
            "{paths}": {{ "{log}": ".log" }},
            "{agent}": {{
                "{config}": "{model_file}",
                "{mode}": "{mode_safe}",
                "{token}": 128000,
                "{iter}": 50,
                "{usd}": 0.5
            }},
            "{tools_k}": {tools}
        }}"#,
            cwd = keys::CWD,
            paths = keys::PATHS,
            log = keys::LOG_FOLDER,
            agent = keys::AGENT_SETTING,
            config = keys::CONFIG,
            mode = keys::MODE,
            mode_safe = keys::MODE_SAFE,
            token = keys::TOKEN_LIMIT,
            iter = keys::ITERATION_LIMIT,
            usd = keys::USD_LIMIT,
            tools_k = keys::TOOLS,
            model_file = model_file,
            tools = tools,
        )
    }

    #[test]
    fn keys_match_schema() {
        assert_eq!(keys::CWD, "cwd");
        assert_eq!(keys::API_KEY, "api-key");
        assert_eq!(keys::TOOLS, "tools");
        assert_eq!(keys::TOOL_FS, "fs");
        assert_eq!(keys::TOOL_GIT, "git");
        assert_eq!(keys::EXECUTABLE, "executable");
        assert_eq!(keys::MODE_SAFE, "safe");
        assert_eq!(keys::MODE_AUTO_ACCEPT, "auto-accept");
        assert_eq!(keys::DEFAULT_LOG_FOLDER, ".log");
    }

    #[test]
    fn load_nested_model_and_tools() {
        let dir = tmp_dir("load");
        write(&dir.join("grok.json5"), &model_body("literal-key"));
        let tools = format!(
            r#"{{
                "{fs}": {{ "{dirs}": ["w:src"], "{ext}": [], "{extra}": ["r:README.md"] }},
                "{git}": {{ "{exe}": "C:/git/git.exe", "{lim}": 30 }},
                "{search}": {{}},
                "{meta}": {{}},
                "{user}": {{}}
            }}"#,
            fs = keys::TOOL_FS,
            dirs = keys::AVAILABLE_DIRS,
            ext = keys::AVAILABLE_FILE_EXTENSIONS,
            extra = keys::EXTRA_SCOPE,
            git = keys::TOOL_GIT,
            exe = keys::EXECUTABLE,
            lim = keys::EXECUTION_LIMIT,
            search = keys::TOOL_SEARCH,
            meta = keys::TOOL_META,
            user = keys::TOOL_USER,
        );
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("grok.json5", &tools));
        let settings = load(&conf).unwrap();
        assert_eq!(settings.cwd, PathBuf::from("C:/workspace"));
        assert_eq!(settings.agent.vendor, "SpaceXAI");
        assert_eq!(settings.agent.name, "Grok 4.6");
        assert_eq!(settings.agent.api_key, "literal-key");
        assert_eq!(settings.agent.token_limit, 128000);
        assert!(settings.agent.mode.is_safe());
        assert!(settings.tools.fs.is_some());
        assert!(settings.tools.git.is_some());
        assert!(settings.tools.search.is_some());
        assert!(settings.tools.terminal.is_none());
        assert_eq!(
            settings.tools.git.as_ref().unwrap().executable,
            "C:/git/git.exe"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_tool_module_is_error() {
        let dir = tmp_dir("unknown");
        write(&dir.join("grok.json5"), &model_body("k"));
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("grok.json5", r#"{ "nope": {} }"#));
        let err = load(&conf).unwrap_err().to_string();
        assert!(err.contains("unknown tool module"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn git_without_executable_is_error() {
        let dir = tmp_dir("git-exe");
        write(&dir.join("grok.json5"), &model_body("k"));
        let conf = dir.join("harness.json5");
        let tools = format!(
            r#"{{ "{git}": {{ "{lim}": 30 }} }}"#,
            git = keys::TOOL_GIT,
            lim = keys::EXECUTION_LIMIT
        );
        write(&conf, &harness_body("grok.json5", &tools));
        let err = load(&conf).unwrap_err().to_string();
        assert!(err.contains(keys::EXECUTABLE), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_api_key_is_error() {
        let dir = tmp_dir("no-key");
        write(
            &dir.join("grok.json5"),
            &format!(
                r#"{{ "{v}": "SpaceXAI", "{n}": "Grok 4.6" }}"#,
                v = keys::VENDOR,
                n = keys::NAME
            ),
        );
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("grok.json5", "{}"));
        let err = load(&conf).unwrap_err().to_string();
        assert!(err.contains(keys::API_KEY), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_api_key_is_error() {
        let dir = tmp_dir("empty-key");
        write(&dir.join("grok.json5"), &model_body("   "));
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("grok.json5", "{}"));
        let err = load(&conf).unwrap_err().to_string();
        assert!(err.contains(keys::API_KEY), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn api_key_from_file() {
        let dir = tmp_dir("key-file");
        let key_file = dir.join("secret.txt");
        write(&key_file, "  file-secret  \n");
        let key_path = key_file.to_string_lossy().replace('\\', "/");
        write(&dir.join("grok.json5"), &model_body(&key_path));
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("grok.json5", "{}"));
        let settings = load(&conf).unwrap();
        assert_eq!(settings.agent.api_key, "file-secret");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_agent_is_error() {
        let dir = tmp_dir("unknown-agent");
        write(
            &dir.join("grok.json5"),
            &format!(
                r#"{{ "{v}": "Nope", "{n}": "X", "{k}": "abc" }}"#,
                v = keys::VENDOR,
                n = keys::NAME,
                k = keys::API_KEY
            ),
        );
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("grok.json5", "{}"));
        let err = load(&conf).unwrap_err().to_string();
        assert!(err.contains("unknown agent"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_session_limits_is_error() {
        let dir = tmp_dir("no-limits");
        write(&dir.join("grok.json5"), &model_body("k"));
        let conf = dir.join("harness.json5");
        write(
            &conf,
            &format!(
                r#"{{
                "{cwd}": "C:/w",
                "{agent}": {{
                    "{config}": "grok.json5",
                    "{mode}": "{mode_safe}"
                }},
                "{tools}": {{}}
            }}"#,
                cwd = keys::CWD,
                agent = keys::AGENT_SETTING,
                config = keys::CONFIG,
                mode = keys::MODE,
                mode_safe = keys::MODE_SAFE,
                tools = keys::TOOLS,
            ),
        );
        let err = load(&conf).unwrap_err().to_string();
        assert!(err.contains("please define"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn conf_new_writes_template_and_refuses_overwrite() {
        let dir = tmp_dir("new");
        let path = dir.join("out.json5");
        write_new_conf(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(&format!("\"{}\"", keys::TOOLS)));
        assert!(text.contains(&format!("\"{}\"", keys::TOOL_FS)));
        assert!(text.contains(&format!("\"{}\"", keys::TOOL_GIT)));
        assert!(text.contains(&format!("\"{}\"", keys::TOOL_SEARCH)));
        assert!(text.contains(&format!("\"{}\"", keys::TOOL_TERMINAL)));
        assert!(text.contains(&format!("\"{}\"", keys::TOOL_META)));
        assert!(text.contains(&format!("\"{}\"", keys::TOOL_USER)));
        assert!(text.contains("//"));
        assert!(text.contains(keys::PLACEHOLDER_GIT_EXECUTABLE));
        assert!(text.contains(keys::PLACEHOLDER_CWD));
        let err = write_new_conf(&path).unwrap_err().to_string();
        assert!(err.contains("already exists"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn conf_new_creates_parent_dirs() {
        let dir = tmp_dir("nested");
        let path = dir.join("a").join("b").join("c.json5");
        write_new_conf(&path).unwrap();
        assert!(path.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn google_model_requires_rpm() {
        let dir = tmp_dir("rpm");
        write(
            &dir.join("g.json5"),
            &format!(
                r#"{{ "{v}": "GoogleAI", "{n}": "Gemini 3.8 Flash", "{k}": "abc" }}"#,
                v = keys::VENDOR,
                n = keys::NAME,
                k = keys::API_KEY
            ),
        );
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("g.json5", "{}"));
        let err = load(&conf).unwrap_err().to_string();
        assert!(err.contains(keys::RPM), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }
}
