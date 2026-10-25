//! All JSON5 keys and load-time default values used by the harness conf crate.
//!
//! Changing a json5 field name means editing this module only. Loaders and
//! tests must read names from these constants. The `--conf-new` template lives
//! in `settings-template.json5` and is checked against these keys in tests.

// ---------------------------------------------------------------------------
// Root object of harness.json5
// ---------------------------------------------------------------------------

/// Workspace of the agent (tools, logs, invoke). Required.
pub const CWD: &str = "cwd";
/// Object with model / mode / session limits and log-folder.
pub const AGENT: &str = "agent";
/// Object whose keys are tool-module names.
pub const TOOLS: &str = "tools";
/// Optional named values substituted before json5 parsing.
pub const VARS: &str = "vars";

// ---------------------------------------------------------------------------
// agent
// ---------------------------------------------------------------------------

/// Path to an agent json5 file or a folder of such files, relative to `--conf`.
pub const CONFIG: &str = "config";
/// Confirmation mode: `safe` or `auto-accept`.
pub const MODE: &str = "mode";
/// Session token limit (`-1` = unlimited).
pub const TOKEN_LIMIT: &str = "token-limit";
/// Session iteration limit (`-1` = unlimited).
pub const ITERATION_LIMIT: &str = "iteration-limit";
/// Session USD limit (`-1` = unlimited).
pub const USD_LIMIT: &str = "USD-limit";
/// Relative log-folder name inside `cwd`.
pub const LOG_FOLDER: &str = "log-folder";

/// `agent.mode` value that asks before exceeding limits.
pub const MODE_SAFE: &str = "safe";
/// `agent.mode` value that only logs limit breaches.
pub const MODE_AUTO_ACCEPT: &str = "auto-accept";

// ---------------------------------------------------------------------------
// Model file
// ---------------------------------------------------------------------------

/// Vendor identifier (`SpaceXAI` / `GoogleAI` / `Claude` / `DeepSeek` / `AliBaba`).
pub const VENDOR: &str = "vendor";
/// Human-readable model name (`Grok 4.6`, `DeepSeek-V3`, `Qwen-Max`, ...).
pub const NAME: &str = "name";
/// API key string or path to a file with the key. Required.
pub const API_KEY: &str = "api-key";
/// Requests per minute inside [`LIMITS`]. `-1` = not enforced.
pub const RPM: &str = "rpm";
/// Tokens per minute inside [`LIMITS`]. `-1` = not enforced.
pub const TPM: &str = "tpm";
/// Requests per day inside [`LIMITS`]. `-1` = not enforced.
pub const RPD: &str = "rpd";
/// Required model-file object: `tokens`, `USD`, `rpm`, `tpm`, `rpd`.
pub const LIMITS: &str = "limits";
/// Token cap inside [`LIMITS`]. `-1` or `>= 1`. Not the session token limit.
pub const LIMIT_TOKENS: &str = "tokens";
/// USD cap inside [`LIMITS`]. `-1` or `>= 0`. Not the session USD limit.
pub const LIMIT_USD: &str = "USD";
/// Required model-file object: input / output / cached prices.
pub const PRICE: &str = "price";
/// USD per 1_000_000 input tokens. `-1` = free.
pub const INPUT_PRICE: &str = "input-price";
/// USD per 1_000_000 output tokens. `-1` = free.
pub const OUTPUT_PRICE: &str = "output-price";
/// USD per 1_000_000 cached tokens. `-1` = free.
pub const CACHED_PRICE: &str = "cached-price";
/// Legacy iteration key. Not allowed in the model file.
pub const ITERATIONS: &str = "iterations";

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
/// Skills catalog module (`get_skill_explanation`).
pub const TOOL_SKILLS: &str = "skills";
/// Roles catalog module (`get_role_explanation`).
pub const TOOL_ROLES: &str = "roles";

// ---------------------------------------------------------------------------
// tools.fs
// ---------------------------------------------------------------------------

/// Directory allow-list with `r:` / `w:` prefixes.
pub const AVAILABLE_DIRS: &str = "available-dirs";
/// Allowed file extensions; empty = any extension.
pub const AVAILABLE_FILE_EXTENSIONS: &str = "available-file-extensions";
/// Exact-file allow-list with `r:` / `w:` prefixes.
pub const AVAILABLE_FILES: &str = "available-files";
/// Former name of [`AVAILABLE_FILES`]. Presence in `tools.fs` is a load error.
pub const EXTRA_SCOPE: &str = "extra-scope";
/// Directory denylist without `r:` / `w:` prefixes.
pub const DEPRECATED_DIRS: &str = "deprecated-dirs";
/// File denylist without `r:` / `w:` prefixes.
pub const DEPRECATED_FILES: &str = "deprecated-files";

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
// tools.skills / tools.roles entries
// ---------------------------------------------------------------------------

/// Short description of a skill or role (embedded into the tool descriptor).
pub const DESCRIPTION: &str = "description";
/// Path to the text file with the full skill or role body.
pub const FILE: &str = "file";

// ---------------------------------------------------------------------------
// Load-time defaults
// ---------------------------------------------------------------------------

/// Default `agent.log-folder`.
pub const DEFAULT_LOG_FOLDER: &str = ".log";
