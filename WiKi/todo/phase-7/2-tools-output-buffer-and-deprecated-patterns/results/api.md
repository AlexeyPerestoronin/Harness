# Phase-7, задача 2: API-изменения tool-вызовов и сигнатур крейте tools

## Было

Schema `run_shell`:

```json
{
  "type": "object",
  "properties": {
    "cwd": {"type": "string"},
    "command": {"type": "string"},
    "arguments": {"type": "string"}
  },
  "required": ["cwd", "command"]
}
```

Schema `request_command_shell_execution`:

```json
{
  "type": "object",
  "properties": {
    "reason": {"type": "string"},
    "command": {"type": "string"},
    "cwd": {"type": "string"}
  },
  "required": ["reason", "command", "cwd"]
}
```

Внутренние сигнатуры:

```rust
pub fn run_observed_command(cmd: &str, cwd: &Path, timeout_secs: u64) -> Result<String, String>;

pub fn Sandbox::from_fs(
    root: PathBuf,
    available_dirs: &[String],
    available_files: &[String],
    extensions: &[String],
    deprecated_dirs: &[String],
    deprecated_files: &[String],
) -> Self;

pub fn FsModule::build(
    settings: Option<&conf::FsSettings>,
    cwd: &Path,
) -> (Option<Self>, Arc<Mutex<Sandbox>>);

pub fn ToolRegistry::build(settings: &conf::ToolsSettings, ctx: ToolsContext) -> Self;
```

## Стало

Schema `run_shell`:

```json
{
  "type": "object",
  "properties": {
    "cwd": {"type": "string"},
    "command": {"type": "string"},
    "arguments": {"type": "string"},
    "output_buff_size": {
      "type": "integer",
      "description": "N символов с конца; -1 = весь вывод"
    }
  },
  "required": ["cwd", "command"]
}
```

Schema `request_command_shell_execution`:

```json
{
  "type": "object",
  "properties": {
    "reason": {"type": "string"},
    "command": {"type": "string"},
    "cwd": {"type": "string"},
    "output_buff_size": {
      "type": "integer",
      "description": "N символов с конца; -1 = весь вывод"
    }
  },
  "required": ["reason", "command", "cwd"]
}
```

Семантика `output_buff_size`:

| Значение | Поведение |
|---|---|
| `-1` | весь вывод возвращается агенту |
| `0` | возвращается пустая строка-результат |
| `N > 0` | возвращаются последние `N` Unicode-символов вывода |
| `< -1` или нецелое | ошибка tool call |

Внутренние сигнатуры:

```rust
pub fn run_observed_command(
    cmd: &str,
    cwd: &Path,
    timeout_secs: u64,
    output_buff_size: i64,
) -> Result<String, String>;

pub fn Sandbox::from_fs(
    root: PathBuf,
    available_dirs: &[String],
    available_files: &[String],
    extensions: &[String],
    deprecated_dirs: &[String],
    deprecated_files: &[String],
    deprecated_dir_patterns: &[String],
) -> Result<Self, String>;

pub fn FsModule::build(
    settings: Option<&conf::FsSettings>,
    cwd: &Path,
) -> Result<(Option<Self>, Arc<Mutex<Sandbox>>), String>;

pub fn ToolRegistry::build(settings: &conf::ToolsSettings, ctx: ToolsContext) -> Result<Self, String>;
```

Добавлены публичные помощники в `tools`:

```rust
pub fn arg_i64(args: &Map<String, Value>, key: &str, default: i64) -> Result<i64, String>;
pub fn validate_output_buff_size(value: i64) -> Result<i64, String>;
```

## Почему

- `output_buff_size` — опциональный, default `-1`, поэтому существующие вызовы `run_shell` и `request_command_shell_execution` остаются совместимыми.
- Полный вывод по-прежнему стримится в консоль пользователя; аргумент ограничивает только строку, возвращаемую агенту, и не меняет поведение процесса.
- `Sandbox::from_fs`, `FsModule::build` и `ToolRegistry::build` стали fallible, чтобы невалидный glob-паттерн, переданный вручную, возвращал ошибку, а не приводил к панике или игнорированию.
- `deprecated_dir_patterns` компилируется в `Vec<Regex>` один раз при создании `Sandbox`; ошибки компиляции пробрасываются наружу.
