//! Configuration crate for the native harness.
//!
//! This is the only crate allowed to open `harness.json5` and nested model
//! files. Callers receive typed [`Settings`] and never parse json5 themselves.
//! All json5 keys live in [`str::keys`]. The `--conf-new` template is a file.

pub mod str;

mod error;
mod fs_scope;
mod load;
mod template;
mod vars;
mod types;

pub use error::ConfError;
pub use load::{load, load_model};
pub use template::{template_text, write_new_conf};
pub use types::{
    AgentKind, AgentSettings, FsSettings, GitSettings, Mode, Settings, TerminalSettings,
    ToolsSettings, UserSettings,
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

    fn fs_obj(dirs: &str, files: &str, dep_dirs: &str, dep_files: &str) -> String {
        format!(
            r#"{{ "{dirs_k}": {dirs}, "{ext}": [], "{files_k}": {files}, "{dd}": {dep_dirs}, "{df}": {dep_files} }}"#,
            dirs_k = keys::AVAILABLE_DIRS,
            ext = keys::AVAILABLE_FILE_EXTENSIONS,
            files_k = keys::AVAILABLE_FILES,
            dd = keys::DEPRECATED_DIRS,
            df = keys::DEPRECATED_FILES,
            dirs = dirs,
            files = files,
            dep_dirs = dep_dirs,
            dep_files = dep_files,
        )
    }

    fn load_fs_obj(label: &str, fs: &str) -> Result<Settings, ConfError> {
        let dir = tmp_dir(label);
        write(&dir.join("grok.json5"), &model_body("k"));
        let tools = format!(r#"{{ "{fs_k}": {fs} }}"#, fs_k = keys::TOOL_FS);
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("grok.json5", &tools));
        let result = load(&conf);
        let _ = fs::remove_dir_all(&dir);
        result
    }

    fn parse_substituted_template() -> serde_json::Value {
        let subst = crate::vars::substitute(&template_text(), "settings-template.json5").unwrap();
        json5::from_str(&subst).expect("substituted template must be valid json5")
    }

    #[test]
    fn keys_match_schema() {
        assert_eq!(keys::CWD, "cwd");
        assert_eq!(keys::API_KEY, "api-key");
        assert_eq!(keys::VARS, "vars");
        assert_eq!(keys::TOOLS, "tools");
        assert_eq!(keys::TOOL_FS, "fs");
        assert_eq!(keys::TOOL_GIT, "git");
        assert_eq!(keys::EXECUTABLE, "executable");
        assert_eq!(keys::AVAILABLE_FILES, "available-files");
        assert_eq!(keys::EXTRA_SCOPE, "extra-scope");
        assert_eq!(keys::DEPRECATED_DIRS, "deprecated-dirs");
        assert_eq!(keys::DEPRECATED_FILES, "deprecated-files");
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
                "{fs}": {{ "{dirs}": ["w:src"], "{ext}": [], "{files}": ["r:README.md"], "{ddirs}": ["target"], "{dfiles}": ["Cargo.lock"] }},
                "{git}": {{ "{exe}": "C:/git/git.exe", "{lim}": 30 }},
                "{user}": {{}}
            }}"#,
            fs = keys::TOOL_FS,
            dirs = keys::AVAILABLE_DIRS,
            ext = keys::AVAILABLE_FILE_EXTENSIONS,
            files = keys::AVAILABLE_FILES,
            ddirs = keys::DEPRECATED_DIRS,
            dfiles = keys::DEPRECATED_FILES,
            git = keys::TOOL_GIT,
            exe = keys::EXECUTABLE,
            lim = keys::EXECUTION_LIMIT,
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
        assert!(settings.tools.terminal.is_none());
        let fs = settings.tools.fs.as_ref().unwrap();
        assert_eq!(fs.available_dirs, vec!["w:src".to_string()]);
        assert!(fs.available_file_extensions.is_empty());
        assert_eq!(fs.available_files, vec!["r:README.md".to_string()]);
        assert_eq!(fs.deprecated_dirs, vec!["target".to_string()]);
        assert_eq!(fs.deprecated_files, vec!["Cargo.lock".to_string()]);
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
    fn search_and_meta_are_unknown_tool_modules() {
        let dir = tmp_dir("search-meta");
        write(&dir.join("grok.json5"), &model_body("k"));
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("grok.json5", r#"{ "search": {} }"#));
        let err = load(&conf).unwrap_err().to_string();
        assert!(err.contains("unknown tool module"), "{err}");
        assert!(err.contains("search"), "{err}");
        write(&conf, &harness_body("grok.json5", r#"{ "meta": {} }"#));
        let err = load(&conf).unwrap_err().to_string();
        assert!(err.contains("unknown tool module"), "{err}");
        assert!(err.contains("meta"), "{err}");
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
    fn load_model_reads_agent_file() {
        let dir = tmp_dir("load-model");
        let path = dir.join("grok.json5");
        write(&path, &model_body("literal-key"));
        let settings = load_model(&path, Mode::AutoAccept).unwrap();
        assert_eq!(settings.vendor, "SpaceXAI");
        assert_eq!(settings.name, "Grok 4.6");
        assert_eq!(settings.api_key, "literal-key");
        assert_eq!(settings.token_limit, 500000);
        assert_eq!(settings.iteration_limit, 50);
        assert!((settings.usd_limit + 1.0).abs() < f64::EPSILON);
        assert!(!settings.mode.is_safe());
        match &settings.kind {
            AgentKind::Xai { api_model } => assert_eq!(api_model, "grok-4.6"),
            other => panic!("expected Xai, got {other:?}"),
        }
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
    fn load_claude_sonnet_5_5() {
        let dir = tmp_dir("claude");
        write(
            &dir.join("claude.json5"),
            &format!(
                r#"{{
                "{v}": "Claude",
                "{n}": "Claude Sonnet 5.5",
                "{k}": "literal-key",
                "{token}": 500000,
                "{iter}": 50,
                "{usd}": -1
            }}"#,
                v = keys::VENDOR,
                n = keys::NAME,
                k = keys::API_KEY,
                token = keys::TOKEN_LIMIT,
                iter = keys::ITERATION_LIMIT,
                usd = keys::USD_LIMIT,
            ),
        );
        let conf = dir.join("harness.json5");
        write(&conf, &harness_body("claude.json5", "{}"));
        let settings = load(&conf).unwrap();
        assert_eq!(settings.agent.vendor, "Claude");
        assert_eq!(settings.agent.name, "Claude Sonnet 5.5");
        assert_eq!(settings.agent.api_key, "literal-key");
        match &settings.agent.kind {
            AgentKind::Claude { api_model } => assert_eq!(api_model, "claude-sonnet-5-5"),
            other => panic!("expected Claude, got {other:?}"),
        }
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
        assert!(text.contains(&format!("\"{}\"", keys::TOOL_TERMINAL)));
        assert!(text.contains(&format!("\"{}\"", keys::TOOL_USER)));
        assert!(text.contains(&format!("\"{}\"", keys::AVAILABLE_FILES)));
        assert!(text.contains(&format!("\"{}\"", keys::DEPRECATED_DIRS)));
        assert!(text.contains(&format!("\"{}\"", keys::DEPRECATED_FILES)));
        assert!(!text.contains(&format!("\"{}\"", keys::EXTRA_SCOPE)));
        assert!(!text.contains("\"search\":"));
        assert!(!text.contains("\"meta\":"));
        assert!(text.contains("find_str"));
        assert!(text.contains("list_available_shell_commands"));
        assert!(text.contains("//"));
        assert!(text.contains("Edit only this section"));
        assert!(text.contains("${vars:"));
        assert!(text.find(&format!("\"{}\"", keys::VARS)).unwrap() < text.find(&format!("\"{}\"", keys::CWD)).unwrap());
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
    fn template_substitutes_to_settings_object() {
        let value = parse_substituted_template();
        let obj = value.as_object().expect("root object");
        for key in [
            keys::VARS,
            keys::CWD,
            keys::PATHS,
            keys::AGENT_SETTING,
            keys::TOOLS,
        ] {
            assert!(obj.contains_key(key), "missing top-level key {key}");
        }
    }

    #[test]
    fn template_vars_are_used_and_declared() {
        let text = template_text();
        let value = parse_substituted_template();
        let vars = value[keys::VARS].as_object().expect("vars object");
        assert!(!vars.is_empty());
        for name in vars.keys() {
            let needle = format!("${{vars:{name}}}");
            assert!(
                text.contains(&needle),
                "variable '{name}' is defined but not used"
            );
        }
        let prefix = "${vars:";
        let mut rest = text.as_str();
        while let Some(i) = rest.find(prefix) {
            let after = &rest[i + prefix.len()..];
            let end = after.find('}').expect("closing brace for ${vars:}");
            let name = &after[..end];
            assert!(
                vars.contains_key(name),
                "variable '{name}' is being used but not defined"
            );
            rest = &after[end..];
        }
    }

    #[test]
    fn template_contains_loader_keys() {
        let text = template_text();
        for key in [
            keys::CWD,
            keys::PATHS,
            keys::LOG_FOLDER,
            keys::AGENT_SETTING,
            keys::CONFIG,
            keys::MODE,
            keys::TOKEN_LIMIT,
            keys::ITERATION_LIMIT,
            keys::USD_LIMIT,
            keys::TOOLS,
            keys::TOOL_FS,
            keys::TOOL_GIT,
            keys::TOOL_TERMINAL,
            keys::TOOL_USER,
            keys::AVAILABLE_DIRS,
            keys::AVAILABLE_FILE_EXTENSIONS,
            keys::AVAILABLE_FILES,
            keys::DEPRECATED_DIRS,
            keys::DEPRECATED_FILES,
            keys::EXECUTABLE,
            keys::EXECUTION_LIMIT,
            keys::CWD_LIST,
            keys::AVAILABLE_COMMANDS,
            keys::COMMAND_EXECUTION_LIMIT,
            keys::INVOKE_EXECUTION_LIMIT,
        ] {
            assert!(
                text.contains(&format!("\"{key}\"")),
                "template is missing key \"{key}\""
            );
        }
    }

    #[test]
    fn template_loads_after_filling_vars() {
        let dir = tmp_dir("template-load");
        let workspace = dir.to_string_lossy().replace('\\', "/");
        write(&dir.join("grok-4.6.json5"), &model_body("literal-key"));
        let text = template_text()
            .replace("<workspace path>", &workspace)
            .replace("path to git-executable file", "git")
            .replace("<work directory>", &workspace);
        let conf = dir.join("harness.json5");
        write(&conf, &text);
        let settings = load(&conf).unwrap();
        assert_eq!(settings.cwd, PathBuf::from(&workspace));
        assert_eq!(settings.agent.api_key, "literal-key");
        assert_eq!(settings.agent.token_limit, 128000);
        assert_eq!(settings.agent.iteration_limit, 50);
        assert!((settings.agent.usd_limit - 0.5).abs() < f64::EPSILON);
        assert!(settings.agent.mode.is_safe());
        assert!(settings.tools.fs.is_some());
        assert!(settings.tools.git.is_some());
        assert!(settings.tools.terminal.is_some());
        assert!(settings.tools.user.is_some());
        let fs = settings.tools.fs.as_ref().unwrap();
        assert_eq!(fs.available_dirs, vec![format!("w:{workspace}")]);
        assert!(fs.available_file_extensions.is_empty());
        assert!(fs.available_files.is_empty());
        assert_eq!(
            fs.deprecated_dirs,
            vec![".git".to_string(), "target".to_string()]
        );
        assert!(fs.deprecated_files.is_empty());
        assert_eq!(settings.tools.git.as_ref().unwrap().executable, "git");
        let term = settings.tools.terminal.as_ref().unwrap();
        assert_eq!(term.cwd_list, vec![workspace.clone()]);
        assert_eq!(term.command_execution_limit, 120);
        assert_eq!(term.invoke_execution_limit, 120);
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

    #[test]
    fn load_harness_and_model_with_vars() {
        let dir = tmp_dir("vars-ok");
        write(
            &dir.join("grok.json5"),
            r#"{
            "vars": {
                "key": "literal-key",
                "iter": 50
            },
            "vendor": "SpaceXAI",
            "name": "Grok 4.6",
            "api-key": "${vars:key}",
            "token-limit": 500000,
            "iteration-limit": ${vars:iter},
            "USD-limit": -1
        }"#,
        );
        let conf = dir.join("harness.json5");
        write(
            &conf,
            r#"{
            "vars": {
                "workspace": "C:/workspace",
                "tokens": 128000,
                "usd": 0.5
            },
            "cwd": "${vars:workspace}",
            "paths": { "log-folder": ".log" },
            "agent-setting": {
                "config": "grok.json5",
                "mode": "safe",
                "token-limit": ${vars:tokens},
                "iteration-limit": 50,
                "USD-limit": ${vars:usd}
            },
            "tools": {}
        }"#,
        );
        let settings = load(&conf).unwrap();
        assert_eq!(settings.cwd, PathBuf::from("C:/workspace"));
        assert_eq!(settings.agent.api_key, "literal-key");
        assert_eq!(settings.agent.token_limit, 128000);
        assert_eq!(settings.agent.iteration_limit, 50);
        assert!((settings.agent.usd_limit - 0.5).abs() < f64::EPSILON);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn vars_do_not_leak_from_model_to_harness() {
        let dir = tmp_dir("vars-leak-h");
        write(
            &dir.join("grok.json5"),
            &format!(
                r#"{{
            "vars": {{ "workspace": "C:/from-model" }},
            {body}
        }}"#,
                body = model_body("literal-key").trim().trim_start_matches('{').trim_end_matches('}'),
            ),
        );
        let conf = dir.join("harness.json5");
        write(
            &conf,
            &harness_body("grok.json5", "{}").replace(
                "\"C:/workspace\"",
                "\"${vars:workspace}\"",
            ),
        );
        let err = load(&conf).unwrap_err().to_string();
        assert!(
            err.contains("variable 'workspace' is being used but not defined"),
            "{err}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn vars_do_not_leak_from_harness_to_model() {
        let dir = tmp_dir("vars-leak-m");
        write(
            &dir.join("grok.json5"),
            &model_body("${vars:key}"),
        );
        let conf = dir.join("harness.json5");
        write(
            &conf,
            r#"{
            "vars": { "key": "literal-key" },
            "cwd": "C:/workspace",
            "paths": { "log-folder": ".log" },
            "agent-setting": {
                "config": "grok.json5",
                "mode": "safe",
                "token-limit": 128000,
                "iteration-limit": 50,
                "USD-limit": 0.5
            },
            "tools": {}
        }"#,
        );
        let err = load(&conf).unwrap_err().to_string();
        assert!(
            err.contains("variable 'key' is being used but not defined"),
            "{err}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_deprecated_lists_are_ok() {
        let settings = load_fs_obj("fs-empty", &fs_obj("[]", "[]", "[]", "[]")).unwrap();
        let fs = settings.tools.fs.unwrap();
        assert!(fs.available_dirs.is_empty());
        assert!(fs.available_files.is_empty());
        assert!(fs.deprecated_dirs.is_empty());
        assert!(fs.deprecated_files.is_empty());
    }

    #[test]
    fn missing_deprecated_keys_is_error() {
        let no_dirs = format!(
            r#"{{ "{dirs}": [], "{ext}": [], "{files}": [], "{dfiles}": [] }}"#,
            dirs = keys::AVAILABLE_DIRS,
            ext = keys::AVAILABLE_FILE_EXTENSIONS,
            files = keys::AVAILABLE_FILES,
            dfiles = keys::DEPRECATED_FILES,
        );
        let err = load_fs_obj("fs-no-ddirs", &no_dirs).unwrap_err().to_string();
        assert!(err.contains(keys::DEPRECATED_DIRS), "{err}");
        assert!(err.contains("empty list"), "{err}");

        let no_files = format!(
            r#"{{ "{dirs}": [], "{ext}": [], "{files}": [], "{ddirs}": [] }}"#,
            dirs = keys::AVAILABLE_DIRS,
            ext = keys::AVAILABLE_FILE_EXTENSIONS,
            files = keys::AVAILABLE_FILES,
            ddirs = keys::DEPRECATED_DIRS,
        );
        let err = load_fs_obj("fs-no-dfiles", &no_files).unwrap_err().to_string();
        assert!(err.contains(keys::DEPRECATED_FILES), "{err}");
        assert!(err.contains("empty list"), "{err}");
    }

    #[test]
    fn extra_scope_was_renamed() {
        let fs = format!(
            r#"{{ "{old}": ["r:README.md"], "{ddirs}": [], "{dfiles}": [] }}"#,
            old = keys::EXTRA_SCOPE,
            ddirs = keys::DEPRECATED_DIRS,
            dfiles = keys::DEPRECATED_FILES,
        );
        let err = load_fs_obj("fs-renamed", &fs).unwrap_err().to_string();
        assert!(err.contains("was renamed to"), "{err}");
        assert!(err.contains(keys::EXTRA_SCOPE), "{err}");
        assert!(err.contains(keys::AVAILABLE_FILES), "{err}");
    }

    #[test]
    fn deprecated_entries_reject_prefix_and_empty() {
        let err = load_fs_obj(
            "fs-dep-prefix",
            &fs_obj("[]", "[]", r#"["r:target"]"#, "[]"),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains(keys::DEPRECATED_DIRS), "{err}");
        assert!(err.contains("r:"), "{err}");

        let err = load_fs_obj(
            "fs-dep-w-prefix",
            &fs_obj("[]", "[]", "[]", r#"["w:Cargo.lock"]"#),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains(keys::DEPRECATED_FILES), "{err}");
        assert!(err.contains("w:"), "{err}");

        let err = load_fs_obj("fs-dep-empty", &fs_obj("[]", "[]", r#"[""]"#, "[]"))
            .unwrap_err()
            .to_string();
        assert!(err.contains(keys::DEPRECATED_DIRS), "{err}");
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn fs_conflicts_are_reported() {
        let err = load_fs_obj(
            "fs-dir-eq",
            &fs_obj(r#"["w:src"]"#, "[]", r#"["src"]"#, "[]"),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("conflicting settings"), "{err}");
        assert!(err.contains(keys::AVAILABLE_DIRS), "{err}");
        assert!(err.contains(keys::DEPRECATED_DIRS), "{err}");
        assert!(err.contains("w:src"), "{err}");
        assert!(err.contains("coincides with"), "{err}");

        let err = load_fs_obj(
            "fs-dir-nested",
            &fs_obj(r#"["w:crates/conf"]"#, "[]", r#"["crates"]"#, "[]"),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains(keys::AVAILABLE_DIRS), "{err}");
        assert!(err.contains(keys::DEPRECATED_DIRS), "{err}");
        assert!(err.contains("w:crates/conf"), "{err}");
        assert!(err.contains("lies inside"), "{err}");

        let err = load_fs_obj(
            "fs-file-eq",
            &fs_obj("[]", r#"["r:README.md"]"#, "[]", r#"["README.md"]"#),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains(keys::AVAILABLE_FILES), "{err}");
        assert!(err.contains(keys::DEPRECATED_FILES), "{err}");
        assert!(err.contains("r:README.md"), "{err}");

        let err = load_fs_obj(
            "fs-file-in-dir",
            &fs_obj("[]", r#"["w:target/foo.txt"]"#, r#"["target"]"#, "[]"),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains(keys::AVAILABLE_FILES), "{err}");
        assert!(err.contains(keys::DEPRECATED_DIRS), "{err}");
        assert!(err.contains("w:target/foo.txt"), "{err}");
        assert!(err.contains("target"), "{err}");
    }

    #[test]
    fn fs_lists_all_conflicts() {
        let err = load_fs_obj(
            "fs-multi",
            &fs_obj(
                r#"["w:src", "w:crates/conf"]"#,
                r#"["r:README.md"]"#,
                r#"["src", "crates"]"#,
                r#"["README.md"]"#,
            ),
        )
        .unwrap_err()
        .to_string();
        let n = err.lines().filter(|l| l.trim_start().starts_with('-')).count();
        assert!(n >= 3, "{err}");
        assert!(err.contains("w:src"), "{err}");
        assert!(err.contains("w:crates/conf"), "{err}");
        assert!(err.contains("r:README.md"), "{err}");
    }

    #[test]
    fn fs_allowed_overlaps_and_separators() {
        load_fs_obj("fs-root-target", &fs_obj(r#"["r:."]"#, "[]", r#"["target"]"#, "[]"))
            .unwrap();
        load_fs_obj(
            "fs-dir-dep-file",
            &fs_obj(r#"["r:crates"]"#, "[]", "[]", r#"["crates/a.txt"]"#),
        )
        .unwrap();
        load_fs_obj(
            "fs-conf-conf2",
            &fs_obj(r#"["w:crates/conf"]"#, "[]", r#"["crates/conf2"]"#, "[]"),
        )
        .unwrap();
        let err = load_fs_obj(
            "fs-slash",
            &fs_obj(r#"["w:crates/conf"]"#, "[]", r#"["crates\\conf"]"#, "[]"),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("conflicting settings"), "{err}");
        assert!(err.contains("w:crates/conf"), "{err}");
    }
}
