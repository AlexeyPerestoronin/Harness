//! Roles tool set (`tools.roles`): return the full text of a named role.
//!
//! Short descriptions from conf are embedded into the tool descriptor so the
//! agent can pick a name. The file is re-read from disk on every call. The path
//! comes from the conf owner, not from the agent, so this module does not use
//! the fs sandbox.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::{arg_str, read_conf_owned_file, CallResult, ToolDescriptor};

/// Roles catalog bound to `tools.roles` entries.
pub struct RolesModule {
    items: BTreeMap<String, conf::RoleEntry>,
}

impl RolesModule {
    /// Construct from typed conf settings.
    pub fn new(settings: conf::RolesSettings) -> Self {
        Self {
            items: settings.items,
        }
    }

    /// JSON Schema descriptor for `get_role_explanation`.
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        vec![ToolDescriptor {
            name: "get_role_explanation".to_string(),
            description: format!(
                "Return the full text of a role by name. Configured roles: {}",
                configured_list(&self.items)
            ),
            parameters: json!({"type":"object","properties":{"role_name":{"type":"string"}},"required":["role_name"]}),
        }]
    }

    /// Dispatch a named roles tool.
    pub fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match name {
            "get_role_explanation" => self.get_role_explanation(&arg_str(args, "role_name")?),
            _ => Err(format!("tool '{name}' not available")),
        }
    }

    fn get_role_explanation(&self, role_name: &str) -> Result<CallResult, String> {
        let entry = self.items.get(role_name).ok_or_else(|| {
            format!(
                "cannot get role explanation → unknown role '{role_name}'; available: {}",
                available_names(&self.items)
            )
        })?;
        let text = read_conf_owned_file(&entry.file, "cannot get role explanation")?;
        Ok(CallResult::plain(text))
    }
}

fn configured_list(items: &BTreeMap<String, conf::RoleEntry>) -> String {
    if items.is_empty() {
        return "none configured".to_string();
    }
    items
        .iter()
        .map(|(name, entry)| format!("{name} — {}", entry.description))
        .collect::<Vec<_>>()
        .join("; ")
}

fn available_names(items: &BTreeMap<String, conf::RoleEntry>) -> String {
    items.keys().cloned().collect::<Vec<_>>().join(", ")
}
