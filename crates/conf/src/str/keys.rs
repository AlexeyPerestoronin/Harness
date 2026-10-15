//! All JSON5 keys and default / placeholder values used by the harness conf crate.
//!
//! Changing a json5 field name means editing this module only. Loaders, the
//! `--conf-new` template and tests must read names from these constants.

// ---------------------------------------------------------------------------
// Root object of harness.json5
// ---------------------------------------------------------------------------

/// Workspace of the agent (tools, logs, invoke). Required.
pub const CWD: &str = "cwd";
/// Object with path-related settings.
pub const PATHS: &str = "paths";
/// Relative log-folder name inside `cwd`.
pub const LOG_FOLDER: &str = "log-folder";
/// Object with model / mode / session limits.
pub const AGENT_SETTING: &str = "agent-setting";
/// Object whose keys are tool-module names.
pub const TOOLS: &str = "tools";

// ---------------------------------------------------------------------------
// agent-setting
// ---------------------------------------------------------------------------

/// Path to the model json5, relative to the directory of `--conf`.
pub const CONFIG: &str = "config";
/// Confirmation mode: `safe` or `auto-accept`.
pub const MODE: &str = "mode";
/// Session token limit (`-1` = unlimited).
pub const TOKEN_LIMIT: &str = "token-limit";
/// Session iteration limit (`-1` = unlimited).
pub const ITERATION_LIMIT: &str = "iteration-limit";
/// Session USD limit (`-1` = unlimited).
pub const USD_LIMIT: &str = "USD-limit";

/// `agent-setting.mode` value that asks before exceeding limits.
pub const MODE_SAFE: &str = "safe";
/// `agent-setting.mode` value that only logs limit breaches.
pub const MODE_AUTO_ACCEPT: &str = "auto-accept";

// ---------------------------------------------------------------------------
// Model file
// ---------------------------------------------------------------------------

/// Vendor identifier (`SpaceXAI` / `GoogleAI`).
pub const VENDOR: &str = "vendor";
/// Human-readable model name (`Grok 4.6`, `Gemini 3.8 Flash`, ...).
pub const NAME: &str = "name";
/// API key string or path to a file with the key. Required.
pub const API_KEY: &str = "api-key";
/// Requests per minute (Google). Required for Google models.
pub const RPM: &str = "rpm";
/// Tokens per minute (Google, recommended).
pub const TPM: &str = "tpm";
/// Requests per day (Google, recommended).
pub const RPD: &str = "rpd";

// ---------------------------------------------------------------------------
// Tool module names (keys of the `tools` object)
// ---------------------------------------------------------------------------

/// Filesystem tools module.
pub const TOOL_FS: &str = "fs";
/// Git tools module.
pub const TOOL_GIT: &str = "git";
/// Shell / invoke tools module.
pub const TOOL_TERMINAL: &str = "terminal";
/// User-interaction tools module.
pub const TOOL_USER: &str = "user";

// ---------------------------------------------------------------------------
// tools.fs
// ---------------------------------------------------------------------------

/// Directory allow-list with `r:` / `w:` prefixes.
pub const AVAILABLE_DIRS: &str = "available-dirs";
/// Allowed file extensions; empty = any extension.
pub const AVAILABLE_FILE_EXTENSIONS: &str = "available-file-extensions";
/// Exact-file allow-list with `r:` / `w:` prefixes.
pub const EXTRA_SCOPE: &str = "extra-scope";

// ---------------------------------------------------------------------------
// tools.git
// ---------------------------------------------------------------------------

/// Path or name of the git executable. Required if `git` is enabled.
pub const EXECUTABLE: &str = "executable";
/// Timeout in seconds for git subprocesses. Required if `git` is enabled.
pub const EXECUTION_LIMIT: &str = "execution-limit";

// ---------------------------------------------------------------------------
// tools.terminal
// ---------------------------------------------------------------------------

/// Directories from which `run_shell` may execute (prefix match on abspath).
pub const CWD_LIST: &str = "cwd-list";
/// Allowed command names without `r:` / `w:` prefixes.
pub const AVAILABLE_COMMANDS: &str = "available-commands";
/// Timeout in seconds for `run_shell`. Required if `terminal` is enabled.
pub const COMMAND_EXECUTION_LIMIT: &str = "command-execution-limit";
/// Timeout in seconds for `run_invoke`. Required if `terminal` is enabled.
pub const INVOKE_EXECUTION_LIMIT: &str = "invoke-execution-limit";

// ---------------------------------------------------------------------------
// Defaults used by `--conf-new` (and log-folder fallback on load)
// ---------------------------------------------------------------------------

/// Default `paths.log-folder`.
pub const DEFAULT_LOG_FOLDER: &str = ".log";
/// Default session token-limit written into the template.
pub const DEFAULT_TOKEN_LIMIT: i64 = 128000;
/// Default session iteration-limit written into the template.
pub const DEFAULT_ITERATION_LIMIT: i64 = 50;
/// Default session USD-limit written into the template.
pub const DEFAULT_USD_LIMIT: f64 = 0.5;
/// Default subprocess timeout (seconds) written into the template.
pub const DEFAULT_EXECUTION_LIMIT: u64 = 120;

// ---------------------------------------------------------------------------
// Placeholders for fields the user must fill in by hand
// ---------------------------------------------------------------------------

/// Stub for `cwd`.
pub const PLACEHOLDER_CWD: &str = "<workspace path>";
/// Stub for `agent-setting.config`.
pub const PLACEHOLDER_MODEL_CONFIG: &str = "grok-4.6.json5";
/// Stub for `tools.git.executable`.
pub const PLACEHOLDER_GIT_EXECUTABLE: &str = "path to git-executable file";
/// Stub for model `api-key`.
pub const PLACEHOLDER_API_KEY: &str = "api-key or path to file with api-key";
/// Stub for a directory in allow-lists / cwd-list.
pub const PLACEHOLDER_DIR: &str = "<absolute or workspace-relative directory>";
/// Stub for a file in extra-scope.
pub const PLACEHOLDER_FILE: &str = "<absolute or workspace-relative file>";
