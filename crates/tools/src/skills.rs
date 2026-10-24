//! Skills tool set (`tools.skills`): return the full text of a named skill.
//!
//! Short descriptions from conf are embedded into the tool descriptor so the
//! agent can pick a name. The file is re-read from disk on every call. The path
//! comes from the conf owner, not from the agent, so this module does not use
//! the fs sandbox.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::{arg_str, read_conf_owned_file, CallResult, ToolDescriptor};

/// Skills catalog bound to `tools.skills` entries.
pub struct SkillsModule {
    items: BTreeMap<String, conf::SkillEntry>,
}

impl SkillsModule {
    /// Construct from typed conf settings.
    pub fn new(settings: conf::SkillsSettings) -> Self {
        Self {
            items: settings.items,
        }
    }

    /// JSON Schema descriptor for `get_skill_explanation`.
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        vec![ToolDescriptor {
            name: "get_skill_explanation".to_string(),
            description: format!(
                "Return the full text of a skill by name. Configured skills: {}",
                configured_list(&self.items)
            ),
            parameters: json!({"type":"object","properties":{"skill_name":{"type":"string"}},"required":["skill_name"]}),
        }]
    }

    /// Dispatch a named skills tool.
    pub fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match name {
            "get_skill_explanation" => self.get_skill_explanation(&arg_str(args, "skill_name")?),
            _ => Err(format!("tool '{name}' not available")),
        }
    }

    fn get_skill_explanation(&self, skill_name: &str) -> Result<CallResult, String> {
        let entry = self.items.get(skill_name).ok_or_else(|| {
            format!(
                "cannot get skill explanation → unknown skill '{skill_name}'; available: {}",
                available_names(&self.items)
            )
        })?;
        let text = read_conf_owned_file(&entry.file, "cannot get skill explanation")?;
        Ok(CallResult::plain(text))
    }
}

fn configured_list(items: &BTreeMap<String, conf::SkillEntry>) -> String {
    if items.is_empty() {
        return "none configured".to_string();
    }
    items
        .iter()
        .map(|(name, entry)| format!("{name} — {}", entry.description))
        .collect::<Vec<_>>()
        .join("; ")
}

fn available_names(items: &BTreeMap<String, conf::SkillEntry>) -> String {
    items.keys().cloned().collect::<Vec<_>>().join(", ")
}
