# Результаты: базовые крейты `logger`, `interactions`, `auth`, `harness-api`

## 1. `crates/logger`

**Было:**
- Трейт `Logger` с двумя обязательными методами: `log_str(&self, message: &str)` и `log_line(&self, message: &str)`.
- Реализации только `DoubleLogger` (stdout + файл) и `NullLogger`.

**Стало:**
- Трейт приведён к целевому контракту `arch.md` §5.1:
  ```rust
  pub trait Logger: Send + Sync {
      fn log_str(&self, message: &str, new_line: bool);
  }
  ```
- `new_line = true` добавляет `\n` перед записью, если `message` не начинается с `\n`; `new_line = false` пишет как есть.
- Устаревший `log_line(&self, message)` оставлен как provided-метод с дефолтом `self.log_str(message, true)` и атрибутом `#[deprecated]`. Это сохранило компиляцию существующего `harness`/`tools` без правок всех вызовов.
- Добавлен `FileLogger::new(path)`: создаёт родительские каталоги, открывает файл на дополнение, пишет только в файл.
- `NullLogger` адаптирован под новый трейт.
- `DoubleLogger` временно сохранён и адаптирован под новый трейт (stdout + файл).

**Почему:** целевой контракт ввода/вывода требует один метод логирования с явным управлением переводом строки; файловый приёмник нужен серверному режиму, а `DoubleLogger` оставлен только для компиляции старого бинарного крейта.

## 2. `crates/harness`

**Было:** вызовы одноаргументного `log_str` в `vendor_request.rs` и `agents/google.rs`.

**Стало:** все вызовы `log_str` переведены на двухаргументную форму; продолжения строк (` fail 429...`, ` fail 503...`, ` → ...`) передают `new_line = false`.

**Почему:** это единственные вызовы старого одноаргументного `log_str`; остальной код использует `log_line`, который остался совместимым.

## 3. Новый крейт `crates/interactions`

**Было:** крейта не существовало.

**Стало:**
- Типы `OutputType`, `InteractionError`, `InteractionKind`, `InteractionRequest` — как в задаче.
- `#[async_trait::async_trait] pub trait Interaction` с методами `input(&InteractionRequest, write_log)` и `output(message, write_log, new_line)`.
- `InteractionLoggers { file: Option<Arc<dyn Logger>> }`.
- `CliInteraction`:
  - хранит `InteractionLoggers`;
  - `output` пишет в stdout по `OutputType` и в файл через `log_str`;
  - `input` печатает промпт, читает stdin через `tokio::task::spawn_blocking`, пишет промпт/ответ в файл только при `write_log = true`;
  - конструктор `with_reader_writer` для инъекции reader/writer.
- Отображение `OutputType` на файловый логгер соответствует `arch.md` §5.4.
- Добавлены модульные тесты для `output`/`input` и режима `write_log = false`.

**Почему:** единая зона ответственности ввода/вывода; серверный режим и web-клиент будут работать через этот контракт.

## 4. Новый крейт `crates/auth`

**Было:** крейта не существовало.

**Стало:**
- Файлы `lib.rs`, `token.rs`, `middleware.rs`, `error.rs`.
- `AuthToken::generate/parse/verify`:
  - генерация 32 случайных байтов, base64url без `=`;
  - `parse` проверяет декодирование и длину 32 байта;
  - `verify` сравнивает байты constant-time (без раннего выхода в цикле).
- `AuthError::{MissingHeader, InvalidScheme, InvalidToken, Unauthorized}` с преобразованием в HTTP `401` и JSON:
  ```json
  { "error": { "code": "UNAUTHORIZED", "message": "missing or invalid bearer token" } }
  ```
- `extract_bearer(headers)` принимает только `Authorization: Bearer <token>`, без query-параметров и cookies.
- Middleware `require_auth` работает со `State<Arc<AuthToken>>`, чтобы не связывать крейт с конкретным `ServerState`.
- Тесты: round-trip генерации/parse/verify, неверный токен, отсутствие заголовка, неверная схема, query-параметр не используется, constant-time совпадение/несовпадение.

**Почему:** auth выделен в отдельный крейт без бизнес-зависимостей; middleware — единственный механизм защиты.

## 5. Новый крейт `crates/harness-api`

**Было:** крейта не существовало.

**Стало:**
- Перечисления `RunState`, `ConfigState`, `PendingInputKind` с serde-переименованиями в camelCase.
- Общие DTO `ErrorBody`, `ErrorResponse`, `HealthResponse`.
- Run DTO: `StartRunRequest`, `StartRunResponse`, `RunSummary`, `PendingInput`, `RunStats`, `RunResult`, `RunStatusResponse`, `StopRunRequest`, `SubmitInputRequest`, `SubmitInputResponse`.
- Config DTO: `ConfigResponse`, `PutConfigRequest`, `PutConfigResponse`, `ConfigInvalidResponse`, `ConfigDefaultResponse`.
- FS DTO: `FsListResponse`, `FsEntry`, `FsReadResponse`, `FsWriteRequest`, `FsWriteResponse`, `FsMkdirResponse`, `FsRemoveResponse`, `FsMoveResponse`.
- `RunStatusResponse` опускает необязательные поля через `skip_serializing_if = "Option::is_none"`.
- `PendingInput.sensitive` имеет serde-значение по умолчанию `false`.
- `run_url` — относительная строка (в DTO это поле; значение задаёт сервер).
- Тесты сериализации/десериализации всех состояний перечислений и `sensitive` по умолчанию.

**Почему:** общий REST-контракт для сервера и клиента; крейт не зависит от axum/leptos/auth, поэтому пригоден и для WASM.

## Проверка

- `cargo build --workspace` — успех.
- `cargo test -p logger -p interactions -p auth -p harness-api` — успех: 3 + 5 + 10 + 5 тестов.
- `cargo test -p harness` — успех (0 тестов в бинарном крейте, компиляция подтверждена).
- `cargo test --workspace` — успех.
- `cargo run -p harness -- --help` — работает как до изменений.

При сборке `harness`/`tools` появляются ожидаемые warnings о вызове устаревшего `log_line`; это временно, до удаления старого бинарного крейта.
