//! Configuration crate for the native harness.
//!
//! This is the only crate allowed to open `harness.json5` and nested model
//! files. Callers receive typed [`Settings`] and never parse json5 themselves.
//! All json5 keys live in [`str::keys`]. The `--conf-new` template is a file.

pub mod str;

mod error;
mod fs_scope;
mod load;
mod scan;
mod template;
mod types;
mod vars;

pub use error::ConfError;
pub use load::{load, load_from_value, load_model, load_with};
pub use template::{template_text, write_new_conf};
pub use types::{
    AgentCandidate, AgentKind, AgentSettings, FsSettings, GitSettings, Mode, RoleEntry,
    RolesSettings, Settings, SkillEntry, SkillsSettings, TerminalSettings, ToolsSettings,
    UserSettings,
};
