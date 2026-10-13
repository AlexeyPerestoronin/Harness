//! Textual JSON5 template for `harness.exe --conf-new`.
//!
//! The template is assembled from [`crate::str::keys`] so renaming a key
//! updates the generated file. Comments are written as source text (not via serde).

use std::fs;
use std::path::Path;

use crate::error::ConfError;
use crate::str::keys;

/// Write a full commented json5 template to `path`.
///
/// Fails if `path` already exists. Creates parent directories as needed.
pub fn write_new_conf(path: &Path) -> Result<(), ConfError> {
    if path.exists() {
        return Err(ConfError::new(format!(
            "conf file already exists: {}",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, template_text())?;
    Ok(())
}

/// Full JSON5 template with comments, defaults and placeholders.
pub fn template_text() -> String {
    format!(
        r#"{{
    // Workspace of the agent (tools, logs, invoke). Must be set explicitly.
    "{cwd}": "{cwd_ph}",
    // Paths used by the loop. `{log_folder}` is relative to `{cwd}`.
    "{paths}": {{
        // Session logs go to `<cwd>/{log_folder}/<YYYY-MM-DD HH-MM>_<vendor>_<model>/`.
        "{log_folder}": "{log_folder_default}"
    }},
    // Model file, confirmation mode and session limits.
    "{agent_setting}": {{
        // Path to the model json5, relative to the directory of this conf file.
        "{config}": "{config_ph}",
        // "{mode_safe}" — confirm when a limit is exceeded; "{mode_auto}" — log and continue.
        "{mode}": "{mode_safe}",
        // Session token limit. `-1` = unlimited. Required (not taken from the model file).
        "{token_limit}": {token_limit_default},
        // Session iteration limit. `-1` = unlimited. Fires on iteration limit+1.
        "{iteration_limit}": {iteration_limit_default},
        // Session USD limit. `-1` = unlimited.
        "{usd_limit}": {usd_limit_default}
    }},
    // Enabled tool modules. Key = module name (`crates/tools/src/<name>.rs`).
    // Missing key → module is not registered. Unknown key → startup error.
    "{tools}": {{
        // Filesystem tools: read/write/create/remove/move file and folder, list_folder.
        "{tool_fs}": {{
            // Directory allow-list. Prefix `r:` (read) or `w:` (write; implies read). Match by abspath prefix.
            "{available_dirs}": [
                "w:{dir_ph}"
            ],
            // Allowed extensions including the dot (".md"). Empty list = any extension.
            "{available_file_extensions}": [
            ],
            // Exact-file allow-list outside `{available_dirs}`. Prefixes `r:` / `w:`.
            "{extra_scope}": [
                "r:{file_ph}"
            ]
        }},
        // Git tools: is_file_under_git, get_file_diff, apply_diff_patch, discard_file_changes.
        "{tool_git}": {{
            // Path or name of the git binary. Required. The code never falls back to "git".
            "{executable}": "{git_ph}",
            // Timeout in seconds for git subprocesses.
            "{execution_limit}": {execution_limit_default}
        }},
        // Search tools: find_str, find_str_by_regex. Uses the shared fs sandbox.
        "{tool_search}": {{}},
        // Shell tools: run_shell, run_invoke.
        "{tool_terminal}": {{
            // Allowed cwd values for run_shell (abspath prefix match), separate from `{available_dirs}`.
            "{cwd_list}": [
                "{dir_ph}"
            ],
            // Command names without r:/w: prefixes. run_invoke does not use this list.
            "{available_commands}": [
                "findstr",
                "cargo"
            ],
            // Timeout in seconds for run_shell. Required if `{tool_terminal}` is enabled.
            "{command_execution_limit}": {execution_limit_default},
            // Timeout in seconds for run_invoke. Required if `{tool_terminal}` is enabled.
            "{invoke_execution_limit}": {execution_limit_default}
        }},
        // Introspection: list_available_* for extensions, dirs, files, shell commands.
        "{tool_meta}": {{}},
        // User interaction: ask_user, request_*_access_for, request_command_shell_execution.
        "{tool_user}": {{}}
    }}
}}
"#,
        cwd = keys::CWD,
        cwd_ph = keys::PLACEHOLDER_CWD,
        paths = keys::PATHS,
        log_folder = keys::LOG_FOLDER,
        log_folder_default = keys::DEFAULT_LOG_FOLDER,
        agent_setting = keys::AGENT_SETTING,
        config = keys::CONFIG,
        config_ph = keys::PLACEHOLDER_MODEL_CONFIG,
        mode = keys::MODE,
        mode_safe = keys::MODE_SAFE,
        mode_auto = keys::MODE_AUTO_ACCEPT,
        token_limit = keys::TOKEN_LIMIT,
        token_limit_default = keys::DEFAULT_TOKEN_LIMIT,
        iteration_limit = keys::ITERATION_LIMIT,
        iteration_limit_default = keys::DEFAULT_ITERATION_LIMIT,
        usd_limit = keys::USD_LIMIT,
        usd_limit_default = keys::DEFAULT_USD_LIMIT,
        tools = keys::TOOLS,
        tool_fs = keys::TOOL_FS,
        available_dirs = keys::AVAILABLE_DIRS,
        available_file_extensions = keys::AVAILABLE_FILE_EXTENSIONS,
        extra_scope = keys::EXTRA_SCOPE,
        dir_ph = keys::PLACEHOLDER_DIR,
        file_ph = keys::PLACEHOLDER_FILE,
        tool_git = keys::TOOL_GIT,
        executable = keys::EXECUTABLE,
        git_ph = keys::PLACEHOLDER_GIT_EXECUTABLE,
        execution_limit = keys::EXECUTION_LIMIT,
        execution_limit_default = keys::DEFAULT_EXECUTION_LIMIT,
        tool_search = keys::TOOL_SEARCH,
        tool_terminal = keys::TOOL_TERMINAL,
        cwd_list = keys::CWD_LIST,
        available_commands = keys::AVAILABLE_COMMANDS,
        command_execution_limit = keys::COMMAND_EXECUTION_LIMIT,
        invoke_execution_limit = keys::INVOKE_EXECUTION_LIMIT,
        tool_meta = keys::TOOL_META,
        tool_user = keys::TOOL_USER,
    )
}
