//! Configuration crate for the native harness.
//!
//! This is the only crate allowed to open `harness.json5` and nested model
//! files. Callers receive typed [`Settings`] / [`ServerSettings`] and never parse json5 themselves.
//! All json5 keys live in [`str::keys`]. The `--conf-new` template is a file.

pub mod glob;
pub mod str;

mod error;
mod fs_scope;
mod load;
mod scan;
mod template;
mod types;
mod vars;

pub use error::{ConfError, ConfErrorKind};
pub use load::{load, load_from_value, load_server_config, load_with, resolve_agent_for_run};
pub use template::{template_text, write_new_conf};
pub use types::{
    AgentCandidate, AgentKind, AgentSession, AgentSettings, AgentSource, ConfigState, FsSettings,
    GitSettings, Mode, ResolvedAgent, RoleEntry, RolesSettings, ServerConfig, ServerConfigState,
    ServerSettings, Settings, SkillEntry, SkillsSettings, TerminalSettings, ToolsSettings,
    UserSettings,
};
