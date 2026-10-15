//! Typed settings produced by the conf crate.
//!
//! Other crates receive these structures and never open json5 files themselves.

use std::path::PathBuf;

/// Confirmation behaviour when a session limit is exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Ask the user before continuing past a limit.
    Safe,
    /// Log the breach and continue.
    AutoAccept,
}

impl Mode {
    /// `true` when limit breaches require an interactive confirmation.
    pub fn is_safe(self) -> bool {
        matches!(self, Mode::Safe)
    }
}

/// Which HTTP agent implementation to construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentKind {
    /// xAI OpenAI-compatible chat completions (`grok-*`).
    Xai {
        /// API model id, e.g. `grok-4.6`.
        api_model: String,
    },
    /// Google `generateContent` REST (`gemini-*`).
    Google {
        /// API model id, e.g. `gemini-3.8-flash`.
        api_model: String,
    },
}

/// Fully resolved harness configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Agent workspace (`cwd` in json5). Tools, logs and invoke run from here.
    pub cwd: PathBuf,
    /// Directory of the loaded `--conf` file (model path is resolved against it).
    pub conf_dir: PathBuf,
    /// Relative log folder inside `cwd`.
    pub log_folder: String,
    /// Agent / model / limits.
    pub agent: AgentSettings,
    /// Enabled tool modules. Missing key = module off.
    pub tools: ToolsSettings,
}

/// Model identity, resolved API key and session limits.
#[derive(Debug, Clone)]
pub struct AgentSettings {
    /// Vendor string from the model file (`SpaceXAI` / `GoogleAI`).
    pub vendor: String,
    /// Display name from the model file (`Grok 4.6`, ...).
    pub name: String,
    /// Already-resolved API key (file contents or the literal string).
    pub api_key: String,
    /// Which agent implementation to start.
    pub kind: AgentKind,
    /// Limit confirmation mode.
    pub mode: Mode,
    /// Session token limit (`-1` = unlimited).
    pub token_limit: i64,
    /// Session iteration limit (`-1` = unlimited).
    pub iteration_limit: i64,
    /// Session USD limit (`-1` = unlimited).
    pub usd_limit: f64,
    /// Requests per minute. Required for Google.
    pub rpm: Option<u32>,
    /// Tokens per minute (informational).
    pub tpm: Option<u32>,
    /// Requests per day (informational).
    pub rpd: Option<u32>,
}

/// Optional settings for each tool module. `None` = module disabled.
#[derive(Debug, Clone, Default)]
pub struct ToolsSettings {
    /// Filesystem tools.
    pub fs: Option<FsSettings>,
    /// Git tools.
    pub git: Option<GitSettings>,
    /// Shell / invoke tools.
    pub terminal: Option<TerminalSettings>,
    /// User-interaction tools.
    pub user: Option<UserSettings>,
}

/// Settings for the `fs` module.
#[derive(Debug, Clone, Default)]
pub struct FsSettings {
    /// Directory allow-list entries with `r:` / `w:` prefixes.
    pub available_dirs: Vec<String>,
    /// Allowed extensions (including the leading dot). Empty = any.
    pub available_file_extensions: Vec<String>,
    /// Exact-file allow-list entries with `r:` / `w:` prefixes.
    pub extra_scope: Vec<String>,
}

/// Settings for the `git` module.
#[derive(Debug, Clone)]
pub struct GitSettings {
    /// Path or name of the git binary. Never defaulted to `"git"` in code.
    pub executable: String,
    /// Subprocess timeout in seconds.
    pub execution_limit: u64,
}

/// Settings for the `terminal` module.
#[derive(Debug, Clone, Default)]
pub struct TerminalSettings {
    /// Allowed cwd values for `run_shell` (prefix match on abspath).
    pub cwd_list: Vec<String>,
    /// Allowed command names without `r:` / `w:` prefixes.
    pub available_commands: Vec<String>,
    /// Timeout for `run_shell` in seconds.
    pub command_execution_limit: u64,
    /// Timeout for `run_invoke` in seconds.
    pub invoke_execution_limit: u64,
}

/// Settings for the `user` module. The module has no fields.
#[derive(Debug, Clone, Default)]
pub struct UserSettings {}
