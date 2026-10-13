//! Introspection tool set (`tools.meta`): list current sandbox and shell allow-lists.
//!
//! Values are read from the live sandbox (so runtime `request_*` grants are visible)
//! and from the terminal whitelist captured at registry build time.

use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::sandbox::Sandbox;
use crate::{CallResult, ToolDescriptor};

/// Introspection tools.
pub struct MetaModule {
    sandbox: Arc<Mutex<Sandbox>>,
    shell_commands: Vec<String>,
}

impl MetaModule {
    /// Bind to the shared sandbox and a snapshot of allowed shell commands.
    pub fn new(sandbox: Arc<Mutex<Sandbox>>, shell_commands: Vec<String>) -> Self {
        Self {
            sandbox,
            shell_commands,
        }
    }

    /// JSON Schema descriptors for the meta tools.
    pub fn descriptors() -> Vec<ToolDescriptor> {
        vec![
            desc("list_available_file_extension", "Список доступных расширений файлов."),
            desc("list_available_r_dir", "Список доступных директорий для чтения."),
            desc("list_available_w_dir", "Список доступных директорий для записи."),
            desc("list_available_r_file", "Список доступных файлов для чтения."),
            desc("list_available_w_file", "Список доступных файлов для записи."),
            desc("list_available_shell_commands", "Список доступных shell команд."),
        ]
    }

    /// Dispatch a named meta tool.
    pub fn call(&self, name: &str, _args: &Map<String, Value>) -> Result<CallResult, String> {
        let sb = self.sandbox.lock().map_err(|e| e.to_string())?;
        let text = match name {
            "list_available_file_extension" => {
                if sb.extensions().is_empty() {
                    "available: all".to_string()
                } else {
                    format!("available: {:?}", sb.extensions())
                }
            }
            "list_available_r_dir" => format!("available: {:?}", sb.r_dirs()),
            "list_available_w_dir" => format!("available: {:?}", sb.w_dirs()),
            "list_available_r_file" => format!("available: {:?}", sb.r_files()),
            "list_available_w_file" => format!("available: {:?}", sb.w_files()),
            "list_available_shell_commands" => {
                format!("available: {:?}", self.shell_commands)
            }
            _ => return Err(format!("tool '{name}' not available")),
        };
        Ok(CallResult::plain(text))
    }
}

fn desc(name: &str, description: &str) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        parameters: json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::Sandbox;

    #[test]
    fn lists_follow_sandbox_and_shell() {
        let sb = Sandbox::from_fs(
            &["w:src".into(), "r:docs".into()],
            &["r:README.md".into()],
            &[".md".into()],
        );
        let m = MetaModule::new(Arc::new(Mutex::new(sb)), vec!["findstr".into()]);
        let ext = m.call("list_available_file_extension", &Map::new()).unwrap();
        assert!(ext.output.contains(".md"), "{}", ext.output);
        let rdir = m.call("list_available_r_dir", &Map::new()).unwrap();
        assert!(rdir.output.contains("src"), "{}", rdir.output);
        assert!(rdir.output.contains("docs"), "{}", rdir.output);
        let wdir = m.call("list_available_w_dir", &Map::new()).unwrap();
        assert!(wdir.output.contains("src"), "{}", wdir.output);
        assert!(!wdir.output.contains("docs"), "{}", wdir.output);
        let sh = m.call("list_available_shell_commands", &Map::new()).unwrap();
        assert!(sh.output.contains("findstr"), "{}", sh.output);
    }
}
