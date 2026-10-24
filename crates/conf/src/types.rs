//! Typed settings produced by the conf crate.
//!
//! Other crates receive these structures and never open json5 files themselves.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::error::ConfError;

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
    /// Anthropic Messages API (`claude-*`).
    Claude {
        /// API model id, e.g. `claude-sonnet-5-5`.
        api_model: String,
    },
}

impl AgentKind {
    /// Map a vendor + display-name pair to a supported implementation.
    pub(crate) fn from_vendor_name(vendor: &str, name: &str) -> Result<Self, ConfError> {
        match (vendor, name) {
            ("SpaceXAI", "Grok 4.6") => Ok(AgentKind::Xai {
                api_model: "grok-4.6".into(),
            }),
            ("GoogleAI", "Gemini 3.8 Flash") => Ok(AgentKind::Google {
                api_model: "gemini-3.8-flash".into(),
            }),
            ("GoogleAI", "Gemini 3.5 Flash Lite") => Ok(AgentKind::Google {
                api_model: "gemini-3.5-flash-lite".into(),
            }),
            ("GoogleAI", "Gemini 3.1 Flash Lite") => Ok(AgentKind::Google {
                api_model: "gemini-3.1-flash-lite".into(),
            }),
            ("Claude", "Claude Sonnet 5.5") => Ok(AgentKind::Claude {
                api_model: "claude-sonnet-5-5".into(),
            }),
            _ => Err(ConfError::new(format!("unknown agent: {vendor} {name}"))),
        }
    }
}

/// An agent json5 file found while scanning a directory.
///
/// Produced without reading `api-key` or checking rate limits; full validation
/// happens only after the caller picks one candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCandidate {
    /// Absolute path to the agent file.
    pub path: PathBuf,
    /// Vendor string from the file (`SpaceXAI` / `GoogleAI` / `Claude`).
    pub vendor: String,
    /// Display name from the file (`Grok 4.6`, ...).
    pub name: String,
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
    /// Conf JSON after vars substitution, with `agent.config` set to the
    /// absolute chosen agent file. Written into dump files as `"settings"`.
    pub source: serde_json::Value,
}

/// Model identity, resolved API key and session limits.
#[derive(Debug, Clone)]
pub struct AgentSettings {
    /// Vendor string from the model file (`SpaceXAI` / `GoogleAI` / `Claude`).
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
    /// Requests per minute (`-1` = not enforced).
    pub rpm: i64,
    /// Tokens per minute (`-1` = not enforced).
    pub tpm: i64,
    /// Requests per day (`-1` = not enforced).
    pub rpd: i64,
    /// Absolute path of the agent json5 file that was loaded.
    pub config_path: PathBuf,
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
    /// Skills catalog (`get_skill_explanation`).
    pub skills: Option<SkillsSettings>,
    /// Roles catalog (`get_role_explanation`).
    pub roles: Option<RolesSettings>,
}

/// Settings for the `fs` module.
#[derive(Debug, Clone, Default)]
pub struct FsSettings {
    /// Directory allow-list entries with `r:` / `w:` prefixes.
    pub available_dirs: Vec<String>,
    /// Allowed extensions (including the leading dot). Empty = any.
    pub available_file_extensions: Vec<String>,
    /// Exact-file allow-list entries with `r:` / `w:` prefixes.
    pub available_files: Vec<String>,
    /// Directory denylist without `r:` / `w:` prefixes. Empty = none excluded.
    pub deprecated_dirs: Vec<String>,
    /// File denylist without `r:` / `w:` prefixes. Empty = none excluded.
    pub deprecated_files: Vec<String>,
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

/// Settings for the `skills` module. `items` is empty when enabled as `{}`.
#[derive(Debug, Clone, Default)]
pub struct SkillsSettings {
    /// Named skills. Keys are `skill_name` values accepted by the tool.
    pub items: BTreeMap<String, SkillEntry>,
}

/// One skill: short description (tool list) and path to the full text.
#[derive(Debug, Clone)]
pub struct SkillEntry {
    /// Non-empty short description embedded into the tool descriptor.
    pub description: String,
    /// Absolute path to the skill file. Re-read on every tool call.
    pub file: PathBuf,
}

/// Settings for the `roles` module. `items` is empty when enabled as `{}`.
#[derive(Debug, Clone, Default)]
pub struct RolesSettings {
    /// Named roles. Keys are `role_name` values accepted by the tool.
    pub items: BTreeMap<String, RoleEntry>,
}

/// One role: short description (tool list) and path to the full text.
#[derive(Debug, Clone)]
pub struct RoleEntry {
    /// Non-empty short description embedded into the tool descriptor.
    pub description: String,
    /// Absolute path to the role file. Re-read on every tool call.
    pub file: PathBuf,
}
