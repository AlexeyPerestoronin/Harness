# Native Agent Harness

[English](README.md)

## Что это

Нативный agent harness на Rust: CLI `harness.exe`, цикл агента, HTTP-агенты xAI (Grok), Google (Gemini) и Claude, реестр инструментов.

## Сборка

Cargo workspace: `crates/conf`, `crates/harness`, `crates/logger`, `crates/tools`.

```bash
cargo test --workspace
cargo build --release --workspace
```

Бинарник: `target/release/harness` (`harness.exe` на Windows).

## CLI

По факту `crates/harness/help.txt`:

```text
harness.exe --help
harness.exe --help-tools
harness.exe --version
harness.exe [--conf <path>] --prompt "<user prompt>"
harness.exe [--conf <path>] --dump "<path to dump file>"
harness.exe --conf-new "<path to new conf file>"
```

Правила:

- `--prompt`, `--dump` и `--conf-new` взаимно исключают друг друга. Один из них обязателен.
- `--conf` необязателен. По умолчанию: `<process-cwd>/harness.json5` (текущая рабочая директория процесса, не `settings.cwd`). Нельзя сочетать с `--conf-new`.
- `--prompt` начинает новую сессию. `--dump` продолжает сессию из dump-файла (промпт берётся из истории dump). `--conf-new` записывает комментированный JSON5-шаблон и завершается.

## Конфиг

### Файл цикла `harness.json5`

- `cwd` — workspace агента (tools, logs, invoke).
- `agent.log-folder` — папка логов относительно `cwd`.
- `agent`:
  - `config` — путь к файлу модели или к папке с такими файлами (будет предложено выбрать). Относительные пути резолвятся от директории этого conf-файла.
  - `mode` — `safe` (спрашивать при превышении лимита) или `auto-accept` (залогировать и продолжить).
  - `token-limit`, `iteration-limit`, `USD-limit` — лимиты сессии; `-1` = без лимита.
- `tools` — объект включённых модулей инструментов.

### Файл модели

- `vendor`, `name`
- `api-key` — сам ключ или путь к файлу с ключом

Известные агенты:

- `SpaceXAI` / `Grok 4.6`
- `GoogleAI` / `Gemini 3.8 Flash`, `Gemini 3.5 Flash Lite`, `Gemini 3.1 Flash Lite`
- `Claude` / `Claude Sonnet 5.5`

Для Google обязателен `rpm`.

## Инструменты

Отсутствующий ключ в `tools` — модуль не регистрируется. Неизвестный ключ — ошибка загрузки. Ключей `search` и `meta` нет.

- **fs** — файлы и папки; `find_str` / `find_str_by_regex`; `find_file_by_str` / `find_files_by_regex`; `list_available_*` для dirs/files/extensions. Поля: `available-dirs`, `available-file-extensions`, `available-files` (префиксы `r:` / `w:` у allow-list директорий и файлов), `deprecated-dirs`, `deprecated-files`.
- **git** — `executable`, `execution-limit`.
- **terminal** — `run_shell`, `run_invoke`, `list_available_shell_commands`; поля `cwd-list`, `available-commands`, таймауты `command-execution-limit` и `invoke-execution-limit`.
- **user** — `ask_user`, `request_read_access_for`, `request_write_access_for`, `request_command_shell_execution`.

## CI

GitHub Actions: push в `master`, Windows и Linux (`windows-latest`, `ubuntu-latest`): `cargo test --workspace` и `cargo build --release --workspace`.
