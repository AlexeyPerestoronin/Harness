# Результат: Серверная конфигурация и двухэтапная загрузка в `conf`

## 1. Краткое описание

Крейт `conf` расширен для серверного режима `harness.exe --serve`:

- добавлен блок `server` и тип `ServerConfig` с валидацией;
- добавлены состояния загрузки `ConfigState` / `ServerConfigState`;
- добавлены `ServerSettings` и `AgentSource`;
- добавлены публичные функции `load_server_config` и `resolve_agent_for_run`;
- `ConfError` расширен явными вариантами `AgentChoiceRequired` и `InvalidAgentChoice`;
- шаблон `settings-template.json5` дополнен блоком `server`;
- добавлены юнит-тесты.

Проверки выполнены успешно:

- `cargo test -p conf` — 10 тестов, все `ok`;
- `cargo build --workspace` — успех.
- `cargo test --workspace` — успех (все крейты).

---

## 2. Изменения в файлах

| Файл | Было | Стало | Почему |
|---|---|---|---|
| `crates/conf/src/str/keys.rs` | Только ключи `cwd` / `agent` / `tools` / `vars`, агентские и инструментальные ключи | Добавлены константы `SERVER`, `HOST`, `PORT`, `LIVE_TIMEOUT`, `INTERACTION_TIMEOUT`, `CORS_ALLOWED_ORIGINS`, `TLS_CERT`, `TLS_KEY` | Все имена JSON5-ключей должны жить в одном месте, чтобы загрузчики и тесты не хардкодили строки |
| `crates/conf/src/types.rs` | Были только типы консольной загрузки: `Settings`, `AgentSettings`, `AgentCandidate`, `ToolsSettings` и т.д. | Добавлены `ConfigState`, `ServerConfig`, `AgentSource`, `AgentSession`, `ServerSettings`, `ServerConfigState`, `ResolvedAgent` | Серверу нужны частичная конфигурация без выбранного агента и явный результат разрешения агента для `POST /run` |
| `crates/conf/src/error.rs` | `ConfError` хранил только строку сообщения | `ConfError` хранит `ConfErrorKind`; добавлены варианты `Message`, `AgentChoiceRequired(Vec<AgentCandidate>)`, `InvalidAgentChoice(String)`; конструкторы `agent_choice_required`, `invalid_agent_choice`; методы `kind`, `is_agent_choice_required`, `is_invalid_agent_choice` | Сервер должен программно отличать «нужно выбрать агента» от обычной ошибки валидации, чтобы запросить выбор через `Interaction` |
| `crates/conf/src/load.rs` | Были только `load`, `load_with`, `load_from_value`, `load_model` и приватные помощники консольной загрузки | Добавлены `load_server_config`, `resolve_agent_for_run`, `server_settings_from_value`, `load_server`, парсеры полей `server`, `choose_from_candidates`, `resolve_api_key_for_run`; часть общей логики выделена в `parse_mode` и `build_agent_settings` | Двухэтапная загрузка: сначала валидация без выбора агента и без `api-key`, затем разрешение конкретного агента только при `POST /run` |
| `crates/conf/src/lib.rs` | Реэкспортировались только консольные типы и функции | Реэкспортированы `ConfErrorKind`, `load_server_config`, `resolve_agent_for_run`, новые типы | Новый публичный API должен быть доступен вызывающим крейтам |
| `crates/conf/settings-template.json5` | Блок `server` отсутствовал | Добавлен блок `server` с комментариями и значениями по умолчанию из `tech.md` §6.1 | `harness.exe --conf-new` должен генерировать актуальную конфигурацию |

---

## 3. Публичный API

```rust
pub enum ConfigState { Ok, Missing, Invalid }

pub struct ServerConfig {
    pub host: String,                         // "127.0.0.1"
    pub port: u16,                            // 4067
    pub live_timeout_minutes: u64,            // 5
    pub interaction_timeout_minutes: u64,     // 10
    pub cors_allowed_origins: Vec<String>,    // []
    pub tls_cert: Option<PathBuf>,            // None
    pub tls_key: Option<PathBuf>,             // None
}

pub enum AgentSource {
    File { path: PathBuf },
    Directory { path: PathBuf, candidates: Vec<AgentCandidate> },
}

pub struct AgentSession {
    pub mode: Mode,
    pub token_limit: i64,
    pub iteration_limit: i64,
    pub usd_limit: f64,
    pub log_folder: String,
}

pub struct ServerSettings {
    pub cwd: PathBuf,
    pub conf_dir: PathBuf,
    pub server: ServerConfig,
    pub agent_source: AgentSource,
    pub agent_session: AgentSession,
    pub tools: ToolsSettings,
    pub source: serde_json::Value,
}

pub enum ServerConfigState {
    Ok(ServerSettings),
    Missing,
    Invalid { message: String },
}

pub struct ResolvedAgent {
    pub agent: AgentSettings,
    pub config_path: PathBuf,
}

pub fn load_server_config(conf_path: &Path) -> Result<ServerConfigState, ConfError>;
pub fn resolve_agent_for_run(
    settings: &ServerSettings,
    agent_choice: Option<&str>,
    api_key_override: Option<&str>,
) -> Result<ResolvedAgent, ConfError>;
```

Пояснение по `agent_session`: поле объединяет режим (`mode`), сессионные лимиты (`token-limit`, `iteration-limit`, `USD-limit`) и `log-folder`. В задаче группа была обозначена как `agent_limits / agent_session`; выбран единый тип `AgentSession`, поскольку он покрывает не только лимиты, но и режим и каталог лога.

---

## 4. Поведение

### 4.1. `load_server_config`

- Файл не существует → `Ok(ServerConfigState::Missing)`.
- Файл повреждён / не парсится / не проходит валидацию → `Ok(ServerConfigState::Invalid { message })`.
- Неожиданная ошибка ввода-вывода (например, нет прав на чтение) → `Err(ConfError)`.
- Валидация включает:
  - подстановку `vars` и разбор JSON5;
  - `cwd`;
  - блок `server` (при отсутствии — значения по умолчанию);
  - блок `tools` существующей логикой `load_tools`;
  - структуру и лимиты блока `agent`;
  - для `agent.config = directory` — `scan_agent_dir` без чтения `api-key` и `limits.USD`;
  - для `agent.config = file` — проверку существования файла; `api-key` не требуется.

### 4.2. `resolve_agent_for_run`

- `agent.config = file`:
  - загружает файл модели;
  - `api_key_override` имеет приоритет;
  - отсутствующий/пустой `api-key` без переопределения возвращает агента с пустым `api_key` (НЕ ошибку), чтобы вызывающий код мог запросить ключ и повторить вызов.
- `agent.config = directory`:
  - ровно один кандидат — автовыбор;
  - несколько кандидатов без выбора — `ConfErrorKind::AgentChoiceRequired`;
  - несколько кандидатов с `agent_choice` — парсинг 1-based номера; `"0"`, `"abc"`, `"99"` → `ConfErrorKind::InvalidAgentChoice`;
  - кандидатов нет — ошибка с перечнем пропущенных файлов и причин.

### 4.3. Совместимость

Существующие `load`, `load_with`, `load_from_value`, `load_model` не изменили публичное поведение. `load_model` по-прежнему требует `api-key` для консольного режима; блок `server` при консольной загрузке игнорируется, как и было.

---

## 5. Тесты

Добавлены юнит-тесты в `crates/conf/src/load.rs`:

1. отсутствующий файл → `Missing`;
2. повреждённый JSON5 → `Invalid { message }`;
3. отсутствующий блок `server` → значения по умолчанию;
4. неизвестный ключ `server` → `Invalid`;
5. `cors-allowed-origins: ["*"]` → `Invalid`;
6. `agent.config = file` без `api-key` → `Ok(ServerSettings)`;
   - `resolve_agent_for_run` с `api_key_override` загружает агента с этим ключом;
   - без переопределения возвращает агента с пустым `api_key`;
7. `agent.config = directory` с одним кандидатом → автовыбор;
8. несколько кандидатов без выбора → `AgentChoiceRequired`;
9. `agent_choice = "2"` выбирает второго; `"0"`, `"abc"`, `"99"` → `InvalidAgentChoice`;
10. `template_text()` содержит блок `server`.

---

## 6. Ручные действия

Не требуются.
