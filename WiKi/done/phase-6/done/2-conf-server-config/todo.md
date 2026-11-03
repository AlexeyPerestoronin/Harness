# Задача на разработку: Серверная конфигурация и двухэтапная загрузка в `conf`

Расширить крейт `conf`: добавить блок `server` (`ServerSettings`), двухэтапную загрузку `load_server_config` / `resolve_agent_for_run`, состояния конфигурации `ok`/`missing`/`invalid` и явное представление «требуется выбор агента».

Зачем: сервер `harness.exe --serve` должен запускаться даже при отсутствующем/повреждённом `harness.json5`, валидировать конфигурацию без выбора агента и без API-ключа, а выбор агента и ввод ключа выполнять только в момент `POST /run`.

Ожидаемый результат: публичный API `conf` позволяет серверу загрузить `ServerSettings` без выбора агента и без `api-key`, затем разрешить конкретного агента через `resolve_agent_for_run`; добавлены юнит-тесты.

## Функциональные требования

### 1. Блок `server` в `harness.json5`

1. Добавить константы ключей в `crates/conf/src/str/keys.rs`:
   - корневой ключ `SERVER: &str = "server"`;
   - поля: `HOST`, `PORT`, `LIVE_TIMEOUT`, `INTERACTION_TIMEOUT`, `CORS_ALLOWED_ORIGINS`, `TLS_CERT`, `TLS_KEY`.

2. Добавить тип `ServerConfig` с полями и значениями по умолчанию (`tech.md` §6.1):
   ```rust
   pub struct ServerConfig {
       pub host: String,                         // "127.0.0.1"
       pub port: u16,                            // 4067
       pub live_timeout_minutes: u64,            // 5
       pub interaction_timeout_minutes: u64,     // 10
       pub cors_allowed_origins: Vec<String>,    // []
       pub tls_cert: Option<PathBuf>,            // None
       pub tls_key: Option<PathBuf>,             // None
   }
   ```

3. Валидация блока `server`:
   - отсутствующий блок `server` → значения по умолчанию;
   - неизвестные ключи блока `server` → ошибка валидации;
   - `port` — целое в диапазоне `1..=65535`;
   - `live-timeout` и `interaction-timeout` — положительные целые (минуты);
   - `cors-allowed-origins` — массив строк; значение `"*"` запрещено (`auth.md` §6);
   - `tls-cert`/`tls-key` — строки или `null`; оба должны быть заданы одновременно, если задан хотя бы один (либо оба `null`).

### 2. Типы для состояния серверной конфигурации

1. `ConfigState` (или эквивалентный перечислимый тип) со значениями `Ok`, `Missing`, `Invalid`.

2. `AgentSource`:
   ```rust
   pub enum AgentSource {
       File { path: PathBuf },
       Directory { path: PathBuf, candidates: Vec<AgentCandidate> },
   }
   ```
   Для `File` путь проверяется на существование, полная валидация модели не выполняется. Для `Directory` используется существующий `scan_agent_dir` (без чтения `api-key` и `limits.USD`).

3. `ServerSettings` — частично загруженная конфигурация без выбранного агента:
   - `cwd: PathBuf`;
   - `conf_dir: PathBuf`;
   - `server: ServerConfig`;
   - `agent_source: AgentSource`;
   - `agent_limits` / `agent_session` — режим (`Mode`), `token-limit`, `iteration-limit`, `USD-limit`, `log-folder`;
   - `tools: ToolsSettings`;
   - `source: serde_json::Value` — исходный корень после подстановки `vars`.

4. `ServerConfigState`:
   ```rust
   pub enum ServerConfigState {
       Ok(ServerSettings),
       Missing,
       Invalid { message: String },
   }
   ```

### 3. `load_server_config`

```rust
pub fn load_server_config(conf_path: &Path) -> Result<ServerConfigState, ConfError>;
```

Поведение:
- файл не существует → `Ok(ServerConfigState::Missing)`;
- файл не читается/не парсится/не проходит валидацию → `Ok(ServerConfigState::Invalid { message })`;
- неожиданная ошибка ввода-вывода (например, нет прав на чтение) → `Err(ConfError)`.

Валидация `Ok(ServerSettings)`:
- выполняется подстановка `vars` и разбор JSON5;
- валидируются `cwd`, блок `server`, блок `tools` (переиспользуя существующую логику), структура и лимиты блока `agent`;
- для `agent.config = directory` сканируется каталог и заполняется `candidates` без выбора;
- для `agent.config = file` проверяется существование файла; `api-key` не требуется;
- `configState = ok` не зависит от выбора агента и наличия `api-key`.

### 4. `resolve_agent_for_run`

```rust
pub fn resolve_agent_for_run(
    settings: &ServerSettings,
    agent_choice: Option<&str>,
    api_key_override: Option<&str>,
) -> Result<ResolvedAgent, ConfError>;
```

Требования:
1. `ResolvedAgent` — структура, содержащая полностью загруженного агента (`AgentSettings`) и, при необходимости, его `config_path`.
2. `agent.config = file`:
   - загрузить файл модели;
   - отсутствующий/пустой `api-key` допускается, только если передан `api_key_override`; если переопределение есть — использовать его значение;
   - если `api-key` отсутствует/пустой и переопределения нет — вернуть агента с пустым `api_key` (НЕ ошибку), чтобы вызывающая сторона могла запросить ключ через `Interaction` и повторить вызов.
3. `agent.config = directory`:
   - ровно один кандидат → выбрать автоматически и загрузить;
   - несколько кандидатов и `agent_choice = None` → вернуть ошибку выбора (см. п.5);
   - несколько кандидатов и `agent_choice = Some(..)` → распарсить 1-based номер; корректный выбор загрузить; некорректный → ошибка неверного выбора (см. п.5);
   - кандидатов нет → ошибка с перечнем пропущенных файлов и причин (как в текущем `load_agent`).
4. `api_key_override` применяется так же, как для `file`, и позволяет загрузить модель с отсутствующим/пустым `api-key`.
5. Расширить `ConfError` (или ввести `ConfErrorKind`) явными вариантами:
   - `AgentChoiceRequired(Vec<AgentCandidate>)` — требуется выбор агента (корректировка H2 Designer-Reviewer);
   - `InvalidAgentChoice(String)` — переданный выбор не является номером из допустимого диапазона;
   - обычные ошибки остаются через существующий `ConfError::new`.
   Обеспечить возможность программно отличить `AgentChoiceRequired` от прочих ошибок.

### 5. Совместимость

1. Существующие `load`, `load_with`, `load_from_value` должны продолжать работать для консольного режима. Блок `server` при их вызове можно игнорировать или заполнять значениями по умолчанию.
2. Функции `load_model`, `scan_agent_dir` и `AgentCandidate` не меняют публичную семантику.

### 6. Шаблон конфигурации

Обновить `crates/conf/settings-template.json5`: добавить блок `server` с комментариями и значениями по умолчанию из `tech.md` §6.1.

## Проверка корректности решения

1. `cargo test -p conf` — успех.
2. Юнит-тесты:
   - отсутствующий файл → `Missing`;
   - повреждённый JSON5 → `Invalid { message }`;
   - блок `server` отсутствует → значения по умолчанию;
   - неизвестный ключ `server` → `Invalid`;
   - `cors-allowed-origins: ["*"]` → `Invalid`;
   - `agent.config = file` без `api-key` → `Ok(ServerSettings)`; `resolve_agent_for_run` с `api_key_override` загружает агента с этим ключом;
   - `agent.config = directory` с одним кандидатом → автовыбор;
   - `agent.config = directory` с несколькими кандидатами и без выбора → `AgentChoiceRequired`;
   - `agent_choice = "2"` выбирает второго кандидата; `"0"`, `"abc"`, `"99"` → `InvalidAgentChoice`;
   - `template_text()` содержит блок `server`.
3. `cargo build --workspace` — успех.

## Scope

Минимально необходимые изменения:

- `crates/conf/**`
- `crates/conf/settings-template.json5`

Справочные материалы для чтения:

- `WiKi/todo/phase-6/consulting 5/base.md`
- `WiKi/todo/phase-6/consulting 5/arch.md`
- `WiKi/todo/phase-6/consulting 5/api.md`
- `WiKi/todo/phase-6/consulting 5/auth.md`
- `WiKi/todo/phase-6/consulting 5/tech.md`
- `WiKi/todo/phase-6/consulting 5/report from designer-reviewer.md`

# Дополнительные указания

1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
