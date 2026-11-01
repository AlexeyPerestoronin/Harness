# Задача на рефакторинг: дедупликация кода запуска сессии в `cli-harness`

## Цель рефакторинга:
Уменьшить количество кода и связность между консольным и серверным запуском агентской сессии. Сейчас `crates/cli-harness/src/main.rs` и `crates/cli-harness/src/server/run.rs` содержат две почти одинаковые реализации одного и того же процесса: выбор агента, запрос API-ключа, формирование `source` с `agent.config` и вспомогательные функции нормализации путей.

## Вносимые изменения:

### Изменение №1: общий модуль запуска сессии
- **Что хотим изменить и почему это лучше:** вынести общие функции в один приватный модуль `crates/cli-harness/src/session.rs`, чтобы консольный и серверный режимы использовали один код. Это устраняет дублирование и уменьшает вероятность расхождений в поведении.
- **Как есть сейчас:**
  - `main.rs` содержит `resolve_agent_for_run_interactive`, `prompt_api_key`, `source_with_agent_config`, `resolve_api_key_override`, `resolve_cwd_override`, `normalize_abs`.
  - `server/run.rs` содержит собственные `resolve_agent_for_run_interactive`, `source_with_agent_config`, `ResolveAgentError`.
  - `server/mod.rs` содержит собственные `resolve_cwd_override`, `normalize_abs`.
- **Как должно быть:**
  - В `crates/cli-harness/src/session.rs` находятся:
    - `pub(crate) enum ResolveAgentError { Cancelled, Failed(String) }` с `Display`;
    - `pub(crate) async fn resolve_agent_for_run_interactive(settings: &ServerSettings, interaction: &Arc<dyn Interaction>) -> Result<conf::ResolvedAgent, ResolveAgentError>`;
    - `pub(crate) async fn prompt_api_key(...) -> Result<String, ResolveAgentError>`;
    - `pub(crate) fn source_with_agent_config(...) -> serde_json::Value`;
    - `pub(crate) fn resolve_api_key_override(...) -> Result<String, String>`;
    - `pub(crate) fn resolve_cwd_override(...) -> Result<PathBuf, String>`;
    - `pub(crate) fn normalize_abs(...) -> Result<PathBuf, String>`.
  - `main.rs` и `server/run.rs` импортируют и используют эти функции, удаляя локальные копии.

### Изменение №2: единая интерактивная резолюция агента
- **Что хотим изменить и почему это лучше:** устранить расхождение между консольным и серверным вариантами выбора агента и запроса ключа.
- **Как есть сейчас:**
  - консольный вариант использует `agent_loop::agent_select::choose`;
  - серверный вариант вручную зацикливает `agent_select::build_prompt` + `agent_select::parse_choice` с отдельной обработкой `InteractionError::Cancelled`.
- **Как должно быть:**
  - `session::resolve_agent_for_run_interactive` реализует один цикл: при `ConfErrorKind::AgentChoiceRequired` — до 3 попыток через `agent_select::build_prompt` / `parse_choice`; при пустом `api-key` — `session::prompt_api_key`; `InteractionError::Cancelled` транслируется в `ResolveAgentError::Cancelled`; после 3 неудачных выборов — `ResolveAgentError::Failed("agent was not selected")`.
  - Консольный режим маппит `ResolveAgentError` в строку ошибки; серверный — различает `Cancelled` (stopped) и `Failed` (failed).

### Изменение №3: удаление ставшей ненужной функции `agent_select::choose`
- **Что хотим изменить и почему это лучше:** после перехода на общий `resolve_agent_for_run_interactive` функция `agent_loop::agent_select::choose` теряет единственного потребителя. Удалить её и связанные с ней тесты, чтобы не оставлять мёртвый публичный API.
- **Как есть сейчас:** `agent_select::choose` использует `build_prompt`/`parse_choice` и имеет собственную обработку single-candidate и retry.
- **Как должно быть:** в `crates/agent-loop/src/agent_select.rs` остаются `MAX_ATTEMPTS`, `build_prompt`, `parse_choice`; `choose` и её тесты удалены. Модульная документация обновлена.

### Изменение №4: общие path-хелперы
- **Что хотим изменить и почему это лучше:** убрать дублирование `resolve_cwd_override` и `normalize_abs`.
- **Как есть сейчас:** одинаковые приватные функции есть в `main.rs` и `server/mod.rs` с незначительными отличиями в тексте ошибок.
- **Как должно быть:** обе точки используют `session::resolve_cwd_override` и `session::normalize_abs`. Внешнее поведение сохраняется.

## Проверка корректности рефакторинга:
1. `cargo build --workspace` — успех, без warnings.
2. `cargo test -p cli-harness -p agent-loop` — успех.
3. Ручная проверка консольного режима: конфигурация с `agent.config = directory` должна запрашивать выбор агента; конфигурация с `agent.config = file` и пустым `api-key` должна запрашивать ключ.
4. Ручная проверка серверного режима: `POST /run` с `agent.config = directory` должен публиковать `pendingInput` для выбора агента; отмена через `POST /stop` должна приводить к `stopped`.
5. Логи и тексты промптов выбора агента/ключа не изменились.

## Scope
Минимально необходимые изменения:

- `crates/cli-harness/src/session.rs` (новый файл)
- `crates/cli-harness/src/main.rs`
- `crates/cli-harness/src/server/mod.rs`
- `crates/cli-harness/src/server/run.rs`
- `crates/agent-loop/src/agent_select.rs`

Справочные материалы для чтения:

- `crates/cli-harness/src/main.rs`
- `crates/cli-harness/src/server/mod.rs`
- `crates/cli-harness/src/server/run.rs`
- `crates/cli-harness/src/server/web_interaction.rs`
- `crates/agent-loop/src/agent_select.rs`
- `crates/conf/src/lib.rs`
- `crates/conf/src/load.rs`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
