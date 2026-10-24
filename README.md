# Native Agent Harness

[Русский](README.ru.md)

## What this is

Native agent harness in Rust: CLI `harness.exe`, an agent loop, HTTP agents for xAI (Grok), Google (Gemini), Claude, DeepSeek and AliBaba (Qwen), and a tool registry.

## Build

Cargo workspace: `crates/conf`, `crates/harness`, `crates/logger`, `crates/tools`.

```bash
cargo test --workspace
cargo build --release --workspace
```

Binary: `target/release/harness` (`harness.exe` on Windows).

## CLI

From `crates/harness/help.txt`:

```text
harness.exe --help
harness.exe --help-tools
harness.exe --version
harness.exe [--conf <path>] --prompt "<user prompt>"
harness.exe [--conf <path>] --prompt-file <path to prompt file>
harness.exe --dump "<path to dump file>"
harness.exe --conf-new "<path to new conf file>"
```

Rules:

- `--prompt`, `--prompt-file`, `--dump` and `--conf-new` are mutually exclusive. One of them is required.
- `--conf` is optional for a new session. Default: `<process-cwd>/harness.json5` (the process working directory, not `settings.cwd`). Cannot be combined with `--conf-new` or `--dump`.
- `--prompt` starts a new session. `--prompt-file` starts a new session with the user prompt read from a UTF-8 file. `--dump` continues from a dump file (the prompt is taken from dump history; configuration is taken from the dump, `--conf` is forbidden). `--conf-new` writes a commented JSON5 template and exits.

## Configuration

### Loop file `harness.json5`

- `cwd` — agent workspace (tools, logs, invoke).
- `agent.log-folder` — log folder relative to `cwd`.
- `agent`:
  - `config` — path to a model file or a folder of model files (you will be asked to choose). Relative paths are resolved against the directory of this conf file.
  - `mode` — `safe` (ask when a limit is exceeded) or `auto-accept` (log and continue).
  - `token-limit`, `iteration-limit`, `USD-limit` — session limits; `-1` = unlimited.
- `tools` — object of enabled tool modules.

### Model file

- `vendor`, `name`
- `api-key` — the key itself, or a path to a file that contains the key
- `rpm`, `tpm`, `rpd` — required integers for every model. `-1` disables that limit; a positive integer enables it.

Known agents:

- `SpaceXAI` / `Grok 4.6`
- `GoogleAI` / `Gemini 3.8 Flash`, `Gemini 3.5 Flash Lite`, `Gemini 3.1 Flash Lite`
- `Claude` / `Claude Sonnet 5.5`
- `DeepSeek` / `DeepSeek-V3`, `DeepSeek-R1`, `DeepSeek-Flash`, `DeepSeek-V4-Pro`
- `AliBaba` / `Qwen-Turbo`, `Qwen-Plus`, `Qwen-Max`

## Tools

A missing key in `tools` means the module is not registered. An unknown key is a load error. There are no `search` or `meta` modules.

- **fs** — files and folders; `find_str` / `find_str_by_regex`; `find_file_by_str` / `find_files_by_regex`; `list_available_*` for dirs, files and extensions. Fields: `available-dirs`, `available-file-extensions`, `available-files` (`r:` / `w:` prefixes on the dir and file allow-lists), `deprecated-dirs`, `deprecated-files`.
- **git** — `executable`, `execution-limit`.
- **terminal** — `run_shell`, `run_invoke`, `list_available_shell_commands`; fields `cwd-list`, `available-commands`, timeouts `command-execution-limit` and `invoke-execution-limit`.
- **user** — `ask_user`, `request_read_access_for`, `request_write_access_for`, `request_command_shell_execution`.
- **skills** / **roles** — the agent can request a skill or role by name (`get_skill_explanation` / `get_role_explanation`).

## CI

GitHub Actions on push to `master`, Windows and Linux (`windows-latest`, `ubuntu-latest`): `cargo test --workspace` and `cargo build --release --workspace`.
