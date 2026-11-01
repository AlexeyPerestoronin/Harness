# Native Agent Harness

[English](README.md)

## Что это

Нативный agent harness на Rust: CLI `harness.exe`, цикл агента, HTTP-агенты xAI (Grok), Google (Gemini), Claude, DeepSeek и AliBaba (Qwen), реестр инструментов, режим HTTPS-сервера и web-клиент на Leptos/WASM (`harness.wasm`).

## Сборка

Cargo workspace: `crates/conf`, `crates/logger`, `crates/tools`, `crates/harness-api`, `crates/interactions`, `crates/agent-loop`, `crates/auth`, `crates/cli-harness`. Крейт `crates/web-harness` исключён из workspace и собирается отдельно через Trunk.

```bash
cargo test --workspace
cargo build --release -p cli-harness
```

Бинарник: `target/release/cli-harness` (`target/release/cli-harness.exe` на Windows). Релизные артефакты публикуются как `harness-<version>-linux`, `harness-<version>-macos` и `harness-<version>.exe`.

## CLI

По факту `crates/cli-harness/help.txt`:

```text
harness.exe --help
harness.exe --help-tools
harness.exe --version
harness.exe [--serve] [--host <addr>] [--port <port>] [--conf <path>] [--cwd <path>]
            [--tls-cert <path> --tls-key <path>] [--auth-token <token>]
harness.exe [--cwd <path>] [--api-key <key>] [--conf <path>] --prompt "<user prompt>"
harness.exe [--cwd <path>] [--api-key <key>] [--conf <path>] --prompt-file <path to prompt file>
harness.exe [--cwd <path>] [--api-key <key>] --dump "<path to dump file>"
harness.exe --conf-new "<path to new conf file>"
```

Правила:

- Без аргументов (или с `--serve`) запускается HTTPS-сервер.
- `--prompt`, `--prompt-file`, `--dump` и `--conf-new` взаимно исключают друг друга. Для консольной сессии один из них обязателен.
- `--conf` необязателен. По умолчанию: `<process-cwd>/harness.json5` для консольных сессий и `<cwd>/harness.json5` для сервера. Нельзя сочетать с `--conf-new` или `--dump`.
- `--prompt` начинает новую сессию. `--prompt-file` начинает новую сессию, промпт читается из UTF-8 файла. `--dump` продолжает сессию из dump-файла (промпт берётся из истории dump; конфигурация берётся из dump, `--conf` запрещён). `--conf-new` записывает комментированный JSON5-шаблон и завершается.
- `--port`, `--host`, `--tls-cert`, `--tls-key` и `--auth-token` допустимы только с `--serve`.

## Режим сервера

`harness.exe --serve` (или без аргументов) запускает HTTPS API-сервер. Порт по умолчанию — `4067`; приоритет: `--port`, затем `server.port` из `harness.json5`, затем `4067`.

Серверные флаги:

- `--host <addr>` — адрес bind, по умолчанию `127.0.0.1`;
- `--port <port>` — HTTPS-порт в диапазоне `1..=65535`;
- `--conf <path>` — путь к `harness.json5` (по умолчанию: `<cwd>/harness.json5`);
- `--cwd <path>` — переопределение корня workspace;
- `--auth-token <token>` — фиксированный Bearer-токен; если не задан, генерируется случайный токен и печатается в stdout;
- `--tls-cert <path>` и `--tls-key <path>` — пользовательские PEM-сертификат/ключ вместо автоматически генерируемого самоподписанного сертификата.

Все эндпоинты `/api/v1/*`, кроме `GET /api/v1/health`, требуют заголовок `Authorization: Bearer <token>`. Если `--auth-token` не задан, токен печатается в stdout при старте сервера.

По умолчанию сервер слушает `127.0.0.1`. Для удалённого доступа укажите `--host 0.0.0.0` (или конкретный интерфейс) и добавьте origin web-клиента в `server.cors-allowed-origins` в `harness.json5`.

При первом старте сервер автоматически генерирует самоподписанный TLS-сертификат и сохраняет его в каталоге данных ОС вне `cwd`.

## Web-клиент

`harness.wasm` — чистое Rust/Leptos CSR-приложение, собираемое Trunk:

```bash
cd crates/web-harness
trunk build --release
```

Результат сборки — `crates/web-harness/dist`. CI публикует его на GitHub Pages при каждом push в `master`.

Клиент:

- подключается к серверу по HTTPS (например, `https://localhost:4067`);
- опрашивает URL-статус сессии каждые 10 секунд;
- показывает состояние сессии, запросы `pendingInput` и итоговый результат;
- **не** показывает лог сессии, **не** выбирает агента и **не** управляет API-ключами — сервер запрашивает их как обычные `pendingInput`.

Самоподписанный сертификат: после первого старта сервера откройте `https://localhost:4067/api/v1/health` в браузере и примите исключение сертификата, прежде чем использовать `harness.wasm` с GitHub Pages.

## Интеграция web-harness

`web-harness` публикуется как Git-тег для source-level интеграции в другие Leptos/WASM-приложения через Cargo/Git. Фича `fs-panel` включает компоненты `ConfigEditor` и `FsBrowser`; standalone-сборка для Pages использует набор фич по умолчанию.

В первой версии `web-harness` не публикуется на crates.io.

## Конфиг

### Файл цикла `harness.json5`

- `cwd` — workspace агента (инструменты и логи).
- `agent.log-folder` — папка логов относительно `cwd`.
- `agent`:
  - `config` — путь к файлу модели или к папке с такими файлами (будет предложено выбрать). Относительные пути резолвятся от директории этого conf-файла.
  - `mode` — `safe` (спрашивать при превышении лимита) или `auto-accept` (залогировать и продолжить).
  - `token-limit`, `iteration-limit`, `USD-limit` — лимиты сессии; `-1` = без лимита.
- `server` — необязательные настройки HTTPS-сервера:
  - `host` — адрес bind, по умолчанию `127.0.0.1`;
  - `port` — порт по умолчанию `4067`;
  - `tls-cert`, `tls-key` — необязательные пути к PEM-сертификату/ключу;
  - `cors-allowed-origins` — явный allowlist origin, по умолчанию `[]`.
- `tools` — объект включённых модулей инструментов.

### Файл модели

- `vendor`, `name`
- `api-key` — сам ключ или путь к файлу с ключом
- `limits` — обязательный объект (`tokens`, `USD`, `rpm`, `tpm`, `rpd`). Нет `limits` — ошибка до старта цикла. `rpm` / `tpm` / `rpd` — целые, `-1` или `>= 1` (`-1` — лимит не учитывается). `tokens` — целое, `-1` или `>= 1`. `USD` — число, `-1` или `>= 0`. Сессионные `token-limit` / `USD-limit` по-прежнему берутся только из `harness.json5`.
- `price` — обязательный объект (`input-price`, `output-price`, `cached-price`), USD за 1_000_000 токенов. `-1` значит бесплатно. Нет `price` — ошибка до старта цикла.
- В файле модели больше нет `iteration-limit`. Итерации сессии задаёт только `harness.json5`.

Известные агенты:

- `SpaceXAI` / `Grok 4.6`
- `GoogleAI` / `Gemini 3.8 Flash`, `Gemini 3.5 Flash Lite`, `Gemini 3.1 Flash Lite`
- `Claude` / `Claude Sonnet 5.5`
- `DeepSeek` / `DeepSeek-V3`, `DeepSeek-R1`, `DeepSeek-Flash`, `DeepSeek-V4-Pro`
- `AliBaba` / `Qwen-Turbo`, `Qwen-Plus`, `Qwen-Max`

## Инструменты

Отсутствующий ключ в `tools` — модуль не регистрируется. Неизвестный ключ — ошибка загрузки. Ключей `search` и `meta` нет.

- **fs** — файлы и папки; `find_str` / `find_str_by_regex`; `find_file_by_str` / `find_files_by_regex`; `list_available_*` для dirs/files/extensions. Поля: `available-dirs`, `available-file-extensions`, `available-files` (префиксы `r:` / `w:` у allow-list директорий и файлов), `deprecated-dirs`, `deprecated-files`.
- **git** — `executable`, `execution-limit`.
- **terminal** — `run_shell`; поля `cwd-list`, `available-commands`, таймаут `command-execution-limit`.
- **user** — `ask_user`, `request_read_access_for`, `request_write_access_for`, `request_command_shell_execution`.
- **skills** / **roles** — агент может запросить skill или role по имени (`get_skill_explanation` / `get_role_explanation`).

## CI

GitHub Actions: push в `master` и теги `v*` / `harness-*`:

- сборка и тесты workspace на `ubuntu-latest`, `windows-latest` и `macos-latest`;
- release-сборка `cli-harness` и загрузка артефактов `harness-<version>.exe`, `harness-<version>-linux` и `harness-<version>-macos`;
- сборка `web-harness` через Trunk и публикация `dist` на GitHub Pages при push в `master`;
- создание GitHub Release с тремя нативными бинарниками при создании тега.
