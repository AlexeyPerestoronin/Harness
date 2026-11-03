# Задача на разработку: Базовые крейты `logger`, `interactions`, `auth`, `harness-api`

Создать фундамент для клиент-серверной архитектуры Phase 6: привести крейт `logger` к целевому контракту, добавить крейты `interactions`, `auth` и `harness-api` с DTO REST-контракта. Существующий бинарный крейт `harness` должен продолжать компилироваться.

Зачем: серверный режим, web-клиент и асинхронный агентский цикл опираются на единый контракт ввода/вывода (`Interaction`), файловый логгер (`Logger`), аутентификацию (`auth`) и общие DTO (`harness-api`). Без них невозможны последующие шаги.

Ожидаемый результат: в workspace добавлены крейты `crates/interactions`, `crates/auth`, `crates/harness-api`; `crates/logger` реализует целевой `Logger` с `FileLogger`/`NullLogger`; `cargo build --workspace` и тесты новых крейтов проходят.

## Функциональные требования

### 1. Крейт `logger`

1. Привести трейт к целевому контракту из `arch.md` §5.1:
   ```rust
   pub trait Logger: Send + Sync {
       fn log_str(&self, message: &str, new_line: bool);
   }
   ```
   Семантика `new_line`:
   - `true` — если `message` не начинается с `\n`, добавить `\n` перед записью;
   - `false` — записать `message` как есть.

2. Для сохранения компиляции существующего кода оставить устаревший provided-метод `log_line(&self, message: &str)` с дефолтной реализацией `self.log_str(message, true)`. Новый код должен использовать `log_str`.

3. Реализации:
   - `FileLogger` — файловый приёмник: `new(path)` создаёт родительские каталоги и открывает файл на дополнение; `log_str` пишет только в файл.
   - `NullLogger` — пустой приёмник.
   - `DoubleLogger` временно оставить для компиляции старого `harness`: реализует новый трейт и продолжает писать в stdout + файл; он будет удалён вместе со старым бинарным крейтом.

4. Обновить вызовы старого одноаргументного `log_str` в `crates/harness/src/vendor_request.rs` и `crates/harness/src/agents/google.rs` на двухаргументную форму с корректным `new_line` (продолжение строки — `false`).

### 2. Крейт `interactions`

Создать `crates/interactions` (lib, зависит от `logger`, `tokio`, `async-trait`).

1. Типы:
   ```rust
   pub enum OutputType { ContinueLine, NewLine, RewriteLine }

   pub enum InteractionError { Cancelled, Timeout, Io(String) }

   pub enum InteractionKind { Text, Confirm }

   pub struct InteractionRequest {
       pub kind: InteractionKind,
       pub prompt: String,
       pub sensitive: bool,
   }
   ```

2. Трейт:
   ```rust
   #[async_trait::async_trait]
   pub trait Interaction: Send + Sync {
       async fn input(
           &self,
           request: &InteractionRequest,
           write_log: bool,
       ) -> Result<String, InteractionError>;

       async fn output(
           &self,
           message: &str,
           write_log: bool,
           new_line: OutputType,
       ) -> Result<(), InteractionError>;
   }
   ```
   `InteractionKind::Text` соответствует `pendingInput.kind = "text"`, `Confirm` — `"confirm"`. Поле `sensitive` — явный признак секретного ввода (корректировка H1 Designer-Reviewer).

3. `InteractionLoggers { file: Option<Arc<dyn Logger>> }` — обёртка над файловым приёмником. Файл пишется только при `write_log = true`; для `input` с `write_log = false` в файл не попадает ни промпт, ни ответ.

4. `CliInteraction`:
   - хранит `InteractionLoggers`;
   - `output` пишет в stdout согласно `OutputType` (`ContinueLine` — `print!`, `NewLine` — с новой строки, `RewriteLine` — `\r{message}`) и в файловый приёмник через `log_str`;
   - `input` печатает `prompt` в stdout, читает строку из stdin через `tokio::task::spawn_blocking`, пишет промпт/ответ в файл только при `write_log = true`;
   - предусмотреть конструктор с инъекцией reader/writer для модульных тестов.

5. Отображение `OutputType` на файловый логгер — как в `arch.md` §5.4.

### 3. Крейт `auth`

Создать `crates/auth` (lib, зависит от `axum`, `serde`, `serde_json`, `rand`).

1. Файлы: `lib.rs`, `token.rs`, `middleware.rs`, `error.rs`.

2. `AuthToken`:
   ```rust
   pub struct AuthToken { secret: [u8; 32] }
   impl AuthToken {
       pub fn generate() -> Self;
       pub fn parse(input: &str) -> Result<Self, AuthError>;
       pub fn verify(&self, candidate: &str) -> bool; // constant-time
   }
   ```
   - генерация: 32 случайных байта, кодирование base64url без `=` (зависимость `base64` с URL_SAFE_NO_PAD либо ручная реализация);
   - `parse` принимает строку, декодирует base64url, проверяет длину 32 байта;
   - `verify` выполняет constant-time сравнение байтов (без раннего выхода).

3. `AuthError`:
   ```rust
   pub enum AuthError { MissingHeader, InvalidScheme, InvalidToken, Unauthorized }
   ```
   с преобразованием в HTTP-ответ `401` и JSON:
   ```json
   { "error": { "code": "UNAUTHORIZED", "message": "missing or invalid bearer token" } }
   ```

4. `extract_bearer(headers) -> Result<&str, AuthError>` принимает ровно одну схему `Bearer <token>` в заголовке `Authorization`. Query-параметры и cookies не рассматриваются.

5. Middleware `require_auth` — axum-middleware, единственный механизм защиты:
   ```rust
   pub async fn require_auth(
       State(state): State<Arc<ServerState>>,
       request: Request<Body>,
       next: Next,
   ) -> Result<Response, (StatusCode, Json<ErrorBody>)>
   ```
   Чтобы не связывать `auth` с конкретным `ServerState`, тип состояния в middleware должен быть определён через generic/собственный типаж или упрощён до `Arc<AuthToken>` + вспомогательная функция. Обработчики не проверяют токен повторно.

6. Тесты: генерация/parse/verify round-trip; неверный токен; отсутствие заголовка; неверная схема; query-параметр не принимается (проверка на уровне extract_bearer); constant-time verify для совпадающего/несовпадающего значения.

### 4. Крейт `harness-api`

Создать `crates/harness-api` (lib, зависит от `serde`, `serde_json`). Крейт не зависит от axum/leptos/auth.

1. Перечисления с serde-переименованиями в camelCase:
   - `RunState`: `idle`, `running`, `waiting_input`, `stopping`, `finished`, `failed`, `stopped`;
   - `ConfigState`: `ok`, `missing`, `invalid`;
   - `PendingInputKind`: `text`, `confirm`.

2. DTO согласно `api.md` §2–§5:
   - `ErrorBody { code, message }`, `ErrorResponse { error: ErrorBody }`;
   - `HealthResponse { status, version, server_time, auth_required, run_state, config_state }`;
   - `StartRunRequest { prompt }`, `StartRunResponse { run_state, run_url }`;
   - `RunSummary { iteration, tokens, usd_approx, usd_real }` (`usd_real` — `Option<f64>`);
   - `PendingInput { request_id, kind, prompt, timeout_at, sensitive }`, где `sensitive` — `bool` с serde-значением по умолчанию `false`;
   - `RunStats { iteration, tokens, usd_approx, usd_real }`;
   - `RunResult { ok, run_state, stats, error }` (`error` — `Option<String>`);
   - `RunStatusResponse { run_state, started_at, updated_at, summary, pending_input, result }`, опциональные поля сериализуются только когда присутствуют (`skip_serializing_if = "Option::is_none"`);
   - `StopRunRequest { reason }` (опционально), `SubmitInputRequest { request_id, answer }`, `SubmitInputResponse { accepted }`;
   - конфигурационные DTO: `ConfigResponse`, `PutConfigRequest`, `PutConfigResponse` (успех `{ ok, path, valid }`), `ConfigDefaultResponse`, отдельный тип/структура для ошибки `CONFIG_INVALID`;
   - файловые DTO: `FsListResponse { path, entries }`, `FsEntry { name, kind, size }`, `FsReadResponse { path, text }`, `FsWriteRequest { text }`, `FsWriteResponse { ok, path }`, `FsMkdirResponse { ok, path }`, `FsRemoveResponse { ok, path }`, `FsMoveResponse { ok, src, dst }`.

3. Все поля `run_url` — относительные строки (например, `"/api/v1/run/current"`).

4. Тесты сериализации/десериализации для всех состояний `RunState`, `ConfigState`, `PendingInputKind`, а также проверка `sensitive` по умолчанию `false`.

### 5. Workspace

Новые крейты создаются в `crates/*`, поэтому автоматически попадают в workspace (`members = ["crates/*"]`). В каждом новом `Cargo.toml` использовать `version.workspace = true`, `edition.workspace = true`.

## Проверка корректности решения

1. `cargo build --workspace` — успех.
2. `cargo test -p logger -p interactions -p auth -p harness-api` — успех.
3. `cargo test -p harness` — существующий бинарный крейт продолжает компилироваться и проходить свои тесты (если они есть).
4. Ручная проверка: `cargo run -p harness -- --help` работает, как до изменений.

## Scope

Минимально необходимые изменения:

- `crates/logger/**`
- `crates/interactions/**` (новый крейт)
- `crates/auth/**` (новый крейт)
- `crates/harness-api/**` (новый крейт)
- `crates/harness/src/vendor_request.rs`
- `crates/harness/src/agents/google.rs`

Справочные материалы для чтения:

- `WiKi/todo/phase-6/consulting 5/base.md`
- `WiKi/todo/phase-6/consulting 5/arch.md`
- `WiKi/todo/phase-6/consulting 5/auth.md`
- `WiKi/todo/phase-6/consulting 5/api.md`
- `WiKi/todo/phase-6/consulting 5/tech.md`
- `WiKi/todo/phase-6/consulting 5/report from designer-reviewer.md`

# Дополнительные указания

1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `manual.md` рядом с `todo.md`.
5. Если что-то непонятно, спроси!
