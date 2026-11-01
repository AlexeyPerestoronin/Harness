# Задача на разработку: Асинхронный агентский цикл, `agent-loop`, `cli-harness` (консольный режим)

Выполнить миграцию агентского цикла на полностью асинхронный стек: перевести `tools` на `Interaction` и `async`, создать библиотечный крейт `crates/agent-loop` с асинхронными агентами и `RequestPipeline`, создать бинарный крейт `crates/cli-harness` с консольным режимом (паритет со старым `harness.exe`) и удалить старый крейт `crates/harness`.

Зачем: серверный режим требует асинхронного `AgentLoop` с отменой через `CancellationToken`, асинхронного `reqwest` и единого контракта `Interaction`; старый синхронный `harness` этому не соответствует.

Ожидаемый результат: workspace собирается без `crates/harness`; `cargo run -p cli-harness -- --prompt "..."` работает как прежний `harness.exe`; `tools` и `agent-loop` асинхронны и готовы к подключению сервера.

## Функциональные требования

### 1. Крейт `tools`: асинхронность и `Interaction`

1. Заменить зависимость на `logger` зависимостью на `interactions` (и `tokio` для `spawn_blocking`/`async`).
2. `ToolsContext`:
   ```rust
   pub struct ToolsContext {
       pub cwd: PathBuf,
       pub temp_dir: PathBuf,
       pub interaction: Arc<dyn Interaction>,
   }
   ```
3. `ToolRegistry`:
   - хранит `Arc<dyn Interaction>` вместо `Arc<dyn Logger>`;
   - `pub async fn call(&self, name: &str, args: Value) -> Result<String, String>`;
   - логирование результатов — через `interaction.output(...).await` с `write_log = true` и корректным `OutputType` (аналог текущих `log_line`/`log_str`).
4. Модули `fs`, `git`, `terminal` остаются синхронными по внутренней логике, но вызываются из `ToolRegistry::call` через `tokio::task::spawn_blocking` (или `spawn_blocking` внутри их async-обёрток). Доступ к `Interaction` из блокирующего контекста не допускается.
5. `UserModule` становится асинхронным и использует `Interaction`:
   - `ask_user` → `Interaction::input` c `kind = Text`, `sensitive = false`, `write_log = true`;
   - `request_read_access_for` / `request_write_access_for` → сначала проверка `deprecated-*` и `resolve_in_root`, затем `Interaction::input` c `kind = Confirm` («Разрешить? [y/n]»), при отказе — второй `input` с `kind = Text` («Причина отказа:»);
   - `request_command_shell_execution` → confirm через `Interaction`, затем запуск команды через `spawn_blocking`;
   - подтверждения трактуются как `y`/`yes`/`д`/`да` (регистронезависимо).
6. Публичный `call` сохраняет прежние правила ошибок (`tool not available`, `missing argument`, `arguments must be an object`).
7. Обновить `help-user.txt`/descriptors при необходимости (только если изменились сигнатуры инструментов).

### 2. Крейт `agent-loop` (новый, lib)

Создать `crates/agent-loop`. Перенести и адаптировать код из `crates/harness/src/{agents,agent_loop.rs,vendor_request.rs,agent_select.rs,dump_settings.rs}`.

1. Зависимости: `conf`, `interactions`, `tools`, `serde`, `serde_json`, `reqwest` (async, `rustls-tls`, `json`), `tokio`, `tokio-util` (CancellationToken), `async-trait`, `uuid`, `chrono`, `rand`.

2. `Agent` trait:
   ```rust
   #[async_trait::async_trait]
   pub trait Agent: Send + Sync {
       fn vendor(&self) -> &str;
       fn model(&self) -> &str;
       fn tokens_limit(&self) -> i64;
       fn consumed_tokens(&self) -> i64;
       fn limit_usd(&self) -> f64;
       fn consumed_usd(&self) -> f64;
       fn consumed_usd_real(&self) -> Option<f64>;
       async fn iteration(&mut self, prompt: Option<&str>) -> Result<bool, String>;
       async fn finish(&self);
       fn dump_session(&self, path: &Path) -> Result<(), String>;
       fn reload_session(&mut self, path: &Path) -> Result<(), String>;
   }
   ```
   `finish` — `async`, так как пишет лог через `Interaction`. `dump_session`/`reload_session` — синхронные файловые операции.

3. Все агенты (`xai`, `google`, `claude`, `deepseek`, `alibaba`, `openai_compat`) используют `Arc<dyn Interaction>` вместо `Arc<dyn Logger>`; логирование — через `interaction.output(...).await`. `invoke_tool` становится `async`.

4. `VendorRequest`:
   - асинхронный `reqwest::Client`;
   - `pub async fn send_json(&mut self, url, headers, body, cancel: &CancellationToken) -> Result<String, String>`;
   - rate-limit: `tokio::time::sleep`, `tokio::time::Instant`; retry-классификация 429/503/transport сохраняется без изменений; `MAX_ATTEMPTS = 24`; 503 → случайная пауза 5..=20 с;
   - активная отправка и чтение тела ответа оборачиваются в `tokio::select!` с `cancel` (по `arch.md` §6.3 и §7.2).

5. `RequestPipeline` (по `arch.md` §6, `tech.md` §3.2):
   - `PipelineContext { attempt, url, started_at, tick, cancel }`;
   - `PipelineError { Cancelled, Transport(String), Hook(String) }`;
   - трейты `PreAction`, `DurAction`, `PostAction` (`async_trait`);
   - трейт `RequestPipeline<T>` с `build`, `pre_actions`, `dur_actions`, `post_actions`, `add_pre/add_dur/add_post` и `execute`;
   - `JsonPipeline<T>` — конкретная JSON-реализация;
   - `VendorRequest` использует `RequestPipeline::execute` для одной попытки; retry/rate-limit остаются в `VendorRequest`;
   - `DurAction`-действия выполняются раз в секунду (`tick = 1s`) и не блокируют отмену.

6. `AgentLoop`:
   - конструктор принимает `cwd`, `log_folder`, `AgentSettings`, `ToolsSettings`, `Arc<dyn Interaction>` и каталог сессии;
   - `pub async fn run(&mut self, prompt: Option<String>, dump_file: &Path, source_dump: Option<&Path>, cancel: CancellationToken, observer: Option<Arc<dyn RunObserver>>) -> Result<RunOutcome, String>`;
   - перед каждой итерацией и перед вендорным запросом проверяет `cancel`;
   - при отмене возвращает `RunOutcome` со состоянием `Stopped`;
   - при ошибке — `Failed` с текстом ошибки; при успешном завершении — `Finished`.

7. Типы состояния (внутренние, без зависимости от `harness-api`):
   ```rust
   pub enum RunState { Running, WaitingInput, Stopping, Finished, Failed, Stopped }
   pub struct RunSummary { pub iteration: i64, pub tokens: i64, pub usd_approx: f64, pub usd_real: Option<f64> }
   pub struct RunOutcome { pub state: RunState, pub summary: RunSummary, pub error: Option<String> }
   ```
   `RunObserver`:
   ```rust
   #[async_trait::async_trait]
   pub trait RunObserver: Send + Sync {
       async fn on_state(&self, state: RunState);
       async fn on_summary(&self, summary: RunSummary);
   }
   ```
   Observer вызывается при смене состояния и при изменении сводки не реже одного раза за итерацию. В консольном режиме observer не используется.

8. `agent_select.rs`:
   - функция построения промпта выбора агента в формате, близком к прежней таблице (`№`, `Vendor`, `Model`, `Cost`, `Limits`);
   - функция разбора 1-based номера; `MAX_ATTEMPTS = 3`;
   - неверный номер/ввод → повторный запрос выбора; после 3 неудач — ошибка «agent was not selected» (корректировка H3 Designer-Reviewer).

9. `dump_settings.rs` переносится без изменения поведения (embed/extract/conf_dir_of).

10. Лимиты сессии: при `Mode::Safe` превышение лимита запрашивает подтверждение через `Interaction::input` c `kind = Confirm`.

11. API-ключ и выбор агента в консольном режиме (см. п.3) запрашиваются через `Interaction`; запрос ключа — `kind = Text`, `sensitive = true`, `write_log = false` (чтобы ключ не попал в `log.md`).

### 3. Крейт `cli-harness` (новый, bin, консольный режим)

Создать `crates/cli-harness`; перенести CLI из `crates/harness/src/{main.rs,cli.rs}` и `crates/harness/help.txt`.

1. `main.rs` — `#[tokio::main] async fn main()`.
2. `cli.rs` сохраняет существующий ручной парсинг и все текущие флаги: `--help`, `--help-tools`, `--version`, `--conf`, `--prompt`, `--prompt-file`, `--dump`, `--cwd`, `--api-key`, `--conf-new`. Правила взаимоисключения и дефолтный `--conf` — как сейчас.
3. Консольный запуск новой сессии:
   - использовать `conf::load_server_config`; состояния `Missing`/`Invalid` → ошибка с понятным сообщением (как прежнее поведение отсутствующего/невалидного конфига);
   - для выбора агента использовать `conf::resolve_agent_for_run` + `agent_select` и `CliInteraction` (повтор до 3 попыток при неверном номере);
   - если у загруженного агента пустой `api-key`, запросить ключ через `CliInteraction` (`sensitive = true`, `write_log = false`) и повторить `resolve_agent_for_run` с `api_key_override`;
   - `--api-key` CLI-флага имеет приоритет как `api_key_override`.
4. Возобновление из dump:
   - `dump_settings::extract` → `conf::load_from_value` (как прежний `main`); выбор агента не требуется, так как в dump хранится абсолютный `agent.config`; `--api-key` работает как override;
   - собрать `AgentLoop` и выполнить `run` с неотменяемым `CancellationToken` и без observer.
5. Файлы сессии: каталог сессии, `log.md`, `dump.json` создаются как раньше (`agent_loop::session_dir`, `dump_settings::embed`).
6. `help.txt` скопировать из старого крейта без изменения текста.

### 4. Workspace и удаление старого крейта

1. Удалить каталог `crates/harness` (все файлы).
2. Так как `members = ["crates/*"]`, новые крейты `crates/agent-loop` и `crates/cli-harness` попадают в workspace автоматически. В их `Cargo.toml` использовать `version.workspace = true`, `edition.workspace = true`.
3. Убедиться, что ни один crate не ссылается на удалённый `harness`.

## Проверка корректности решения

1. `cargo build --workspace` — успех.
2. `cargo test -p tools -p agent-loop -p cli-harness` — успех.
3. Юнит-тесты:
   - `RequestPipeline`: отмена через `CancellationToken` прерывает выполнение pre/dur/post цикла и возвращает `PipelineError::Cancelled`;
   - `VendorRequest`: retry-классификация (429/503/transport/2xx/4xx), rate-limit не нарушает отмену;
   - `agent_select`: корректный номер, неверный номер, исчерпание попыток;
   - `tools::UserModule`: подтверждение `y`/`yes`/`д`/`да`, отказ и запрос причины (на mock `Interaction`);
   - лог не содержит введённый API-ключ (mock `Interaction` c `write_log = false`).
4. Ручная проверка консольного режима:
   - `cargo run -p cli-harness -- --help` и `--version`;
   - `cargo run -p cli-harness -- --conf-new "путь"`;
   - запуск с локальным конфигом и коротким промптом (если доступен API-ключ); либо проверка на конфиге с отсутствующим ключом, что запрашивается ключ.
5. `git status` — `crates/harness` удалён, workspace собирается.

## Scope

Минимально необходимые изменения:

- `crates/tools/**`
- `crates/agent-loop/**` (новый крейт)
- `crates/cli-harness/**` (новый крейт)
- `crates/harness/**` (удаление)

Справочные материалы для чтения:

- `WiKi/todo/phase-6/consulting 5/base.md`
- `WiKi/todo/phase-6/consulting 5/arch.md`
- `WiKi/todo/phase-6/consulting 5/tech.md`
- `WiKi/todo/phase-6/consulting 5/api.md`
- `WiKi/todo/phase-6/consulting 5/ui.md`
- `WiKi/todo/phase-6/consulting 5/auth.md`
- `WiKi/todo/phase-6/consulting 5/report from designer-reviewer.md`

# Дополнительные указания

1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
