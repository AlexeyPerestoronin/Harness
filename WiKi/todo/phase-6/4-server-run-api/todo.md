# Задача на разработку: HTTPS-сервер `cli-harness`: запуск, статус, остановка и взаимодействие

Добавить в `crates/cli-harness` режим HTTPS-сервера (`--serve`/без параметров) с аутентификацией, TLS, состоянием сервера и endpoint'ами запуска/статуса/остановки/ввода (`/api/v1/run*`). Реализовать `WebInteraction`, `ServerState`, `live-timeout`, `interaction-timeout`, CORS и интеграцию с `auth`.

Зачем: серверный режим — ядро Phase 6; он должен запускать не более одной активной сессии, управлять её жизненным циклом и передавать запросы на ввод web-клиенту через polling.

Ожидаемый результат: `harness.exe --serve` (или без параметров) поднимает HTTPS-сервер; `GET /health` открыт; все `/api/v1/run*` защищены Bearer-токеном; цикл запускается, опрашивается, останавливается и принимает ответы пользователя.

## Функциональные требования

### 1. CLI серверного режима

1. Добавить в `cli.rs` флаги:
   - `--serve` — запуск в режиме сервера;
   - `--port <n>` — порт (только с `--serve`);
   - `--host <addr>` — адрес bind (по умолчанию `127.0.0.1`);
   - `--tls-cert <path>` / `--tls-key <path>` — пользовательские TLS-файлы (только с `--serve`);
   - `--auth-token <token>` — фиксированный Bearer-токен (только с `--serve`).
2. Режимы:
   - `harness.exe` без аргументов → серверный режим с параметрами по умолчанию;
   - `harness.exe --serve [опции]` → серверный режим;
   - консольные флаги (`--prompt`, `--prompt-file`, `--dump`, `--conf-new`) работают как раньше.
3. Приоритет порта: `--port` → `server.port` из конфигурации → `4067`.
4. Если `--cwd` не задан — используется текущая рабочая директория процесса; `--conf` по умолчанию — `<cwd>/harness.json5`.
5. Обновить `help.txt`.

### 2. TLS

1. Использовать `axum-server` c фичей `tls-rustls` + `rustls` + `rcgen`.
2. Если `--tls-cert`/`--tls-key` или `server.tls-cert`/`server.tls-key` заданы — использовать указанные PEM-файлы; автогенерация не выполняется.
3. Иначе автоматически генерировать самоподписанный сертификат (`subject_alt_names`: `localhost`, `127.0.0.1`) и сохранять `cert.pem`/`key.pem` в каталог данных ОС вне `cwd`:
   - Windows: `%LOCALAPPDATA%\harness\certs\`;
   - Linux: `$XDG_DATA_HOME/harness/certs/` или `~/.local/share/harness/certs/`;
   - macOS: `~/Library/Application Support/harness/certs/`.
4. Каталог создавать при первом старте; при повторном запуске переиспользовать существующие сертификат и ключ.
5. На Linux/macOS: каталог — права `0700`, `key.pem` — `0600` (корректировка §5.1 Designer-Reviewer).
6. TLS-артефакты физически вне `cwd`, поэтому FS API не сможет их прочитать.

### 3. Аутентификация и CORS

1. При старте без `--auth-token` генерировать `AuthToken` (32 байта, base64url) и печатать токен в stdout.
2. Собрать роутер:
   - открытый: `GET /api/v1/health`;
   - защищённые: `POST /api/v1/run`, `GET /api/v1/run/current`, `POST /api/v1/run/stop`, `POST /api/v1/run/input`;
   - на защищённую группу вешается `auth::require_auth` одним слоем; обработчики токен повторно не проверяют.
3. CORS:
   - `server.cors-allowed-origins` пуст → сервер не отправляет `Access-Control-Allow-Origin`;
   - непуст → отправлять CORS-заголовки только при точном совпадении `Origin`; preflight: `Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS`, `Access-Control-Allow-Headers: Authorization, Content-Type`.
4. Формат ошибки — `api.md` §1: `{ "error": { "code", "message" } }`.

### 4. `ServerState` и `ActiveRun`

1. Реализовать `ServerState` (только `tokio::sync`):
   ```rust
   struct ServerState {
       config: tokio::sync::RwLock<LoadedServerConfig>,
       active: tokio::sync::Mutex<Option<ActiveRun>>,
       auth_token: AuthToken,
       live_timeout: Duration,
       live_deadline: tokio::sync::Mutex<Instant>,
   }
   ```
2. `ActiveRun`:
   ```rust
   struct ActiveRun {
       run_id: String,
       started_at: DateTime<Utc>,
       updated_at: DateTime<Utc>,
       state: RunState,            // running | waiting_input | stopping | finished | failed | stopped
       summary: RunSummary,
       pending_input: Option<PendingInput>,
       result: Option<RunResult>,
       cancel: CancellationToken,
       input_tx: Option<oneshot::Sender<UserResponse>>,
   }
   ```
   `UserResponse` — тип ответа пользователя (например, `Answer(String)`).
3. `LoadedServerConfig` хранит путь к `harness.json5`, `config_state` (`ok`/`missing`/`invalid`), `ServerSettings` (если валиден), параметры `server`, описание источника агентов без выбора конкретного агента.
4. `config_state` вычисляется через `conf::load_server_config` при старте сервера; в серверном режиме `Missing`/`Invalid` не являются причиной отказа запуска.

### 5. `WebInteraction`

Реализовать `WebInteraction` в `crates/cli-harness/src/server/`:

1. Реализует `Interaction`; файловый лог — через `FileLogger`.
2. `output` пишет в файл сессии только при `write_log = true`; stdout не используется.
3. `input`:
   - формирует `PendingInput` с `requestId` (ULID с префиксом `req_`, 26 символов после префикса), `kind` (из `InteractionRequest.kind`), `prompt`, `timeoutAt` (момент `interaction-timeout`), `sensitive`;
   - сохраняет `pending_input` в `ActiveRun` и переводит `state` в `waiting_input`;
   - ожидает ответ через `oneshot` с таймаутом `server.interaction-timeout` (по умолчанию 10 минут);
   - по таймауту возвращает `InteractionError::Timeout`; по отмене — `InteractionError::Cancelled`;
   - факт запроса и ответ логируются в файл только при `write_log = true` (ввод API-ключа идёт с `write_log = false`).
4. Промпт запроса `input` не пишется в файл автоматически; файл получает только то, что разрешено флагом `write_log`.

### 6. Endpoint'ы жизненного цикла

По `api.md` §2–§3:

1. `GET /api/v1/health` (открытый):
   - `200 { status: "ok", version: "1.5.0", serverTime, authRequired: true, runState, configState }`;
   - не возвращает `cwd`, `port`, `liveTimeoutMinutes`.
2. `POST /api/v1/run`:
   - пустой `prompt` → `400 INVALID_PROMPT`;
   - конфигурация `missing`/`invalid` → `412`;
   - активный цикл (`running`/`waiting_input`/`stopping`) → `409`;
   - иначе создать `ActiveRun`, запустить tokio-задачу и вернуть `202 { runState: "running", runUrl: "/api/v1/run/current" }`;
   - `409` возвращается только для `running`/`waiting_input`/`stopping`; из `finished`/`failed`/`stopped` новый запуск допустим (корректировка §5.4 Designer-Reviewer).
3. `GET /api/v1/run/current`:
   - нет запуска → `200 { runState: "idle" }`;
   - активный/завершённый — согласно `api.md` §3 (summary, pendingInput, result в соответствующих состояниях);
   - терминальное состояние сохраняется до следующего `POST /run`.
4. `POST /api/v1/run/stop` (идемпотентный):
   - активен → отменить `CancellationToken`, вернуть `202 { runState: "stopping" }`;
   - уже `stopping`/`stopped` → `202 { runState: "stopping" }`;
   - нет активного → `200 { runState: "idle" }`.
5. `POST /api/v1/run/input`:
   - `kind = text` и пустой `answer` → `400`;
   - `kind = confirm` и `answer` не `y`/`n`/`yes`/`no` → `400`;
   - нет ожидающего ввода с таким `requestId` → `404`;
   - ввод уже отвечен/отменён/истёк → `409`;
   - иначе передать ответ в `oneshot` и вернуть `202 { accepted: true }`.
6. При отмене/таймауте/ошибке/завершении цикла заполнять `ActiveRun.state` и `result` согласно `api.md` §3 и `arch.md` §8.4.

### 7. Запуск агентского цикла на сервере

В задаче, запускаемой `POST /run`:

1. Использовать `LoadedServerConfig` из `ServerState` (полученный при старте через `conf::load_server_config`) и `conf::resolve_agent_for_run`:
   - `AgentChoiceRequired(candidates)` → сформировать промпт выбора (`agent_select`), запросить через `WebInteraction` (`kind = Text`, `sensitive = false`); неверный номер → повторный запрос (до 3 попыток), затем `failed` с понятной ошибкой;
   - пустой `api-key` → запросить ключ через `WebInteraction` (`kind = Text`, `sensitive = true`, `write_log = false`) и повторить `resolve_agent_for_run` с `api_key_override`.
2. Создать каталог сессии, `log.md`, `dump.json`; собрать `ToolRegistry` с `WebInteraction`; создать `AgentLoop` и запустить `run` с `CancellationToken` из `ActiveRun`.
3. Передавать `RunObserver`, который обновляет `ActiveRun.summary`/`state`/`updated_at`; при завершении — `result`.
4. `live-timeout` приостанавливается на время активного цикла.

### 8. Таймауты

1. `live-timeout`:
   - таймер стартует при запуске сервера;
   - мутирующие вызовы (`POST /run`, `POST /run/stop`, `POST /run/input`) сбрасывают `live_deadline`;
   - polling (`GET /health`, `GET /run/current`) и `GET /config`, `GET /fs/*` НЕ продлевают таймер;
   - по истечении сервер завершает процесс с кодом 0;
   - на время активного цикла отсчёт приостановлен.
2. `interaction-timeout`:
   - отсчитывается от сохранения `pending_input`;
   - по истечении `WebInteraction::input` возвращает `Timeout`, цикл завершается `failed`, в `result` — ошибка таймаута.

## Проверка корректности решения

1. `cargo build --workspace` и `cargo test -p cli-harness` — успех.
2. Юнит-тесты:
   - парсинг CLI серверных флагов (включая запрет `--port` без `--serve`);
   - `WebInteraction` сохраняет `pending_input` с корректным `kind`/`sensitive`/`timeoutAt`;
   - валидация `POST /run/input` (`text` пустой, `confirm` некорректный, `404`, `409`);
   - health не раскрывает `cwd`/`port`/`liveTimeoutMinutes`;
   - генерация/переиспользование TLS-артефактов; на Unix — права `0700`/`0600`;
   - router: `GET /run/current` без токена → `401`, с токеном → `200`, health без токена → `200`.
3. Ручная проверка локального сервера:
   - `cargo run -p cli-harness -- --serve --port 4067`;
   - токен печатается в stdout;
   - `curl -k https://localhost:4067/api/v1/health`;
   - `curl -k -H "Authorization: Bearer <token>" https://localhost:4067/api/v1/run/current`;
   - запуск/остановка/ввод по сценариям `api.md` §3.

## Scope

Минимально необходимые изменения:

- `crates/cli-harness/**`

Справочные материалы для чтения:

- `WiKi/todo/phase-6/consulting 5/base.md`
- `WiKi/todo/phase-6/consulting 5/arch.md`
- `WiKi/todo/phase-6/consulting 5/tech.md`
- `WiKi/todo/phase-6/consulting 5/api.md`
- `WiKi/todo/phase-6/consulting 5/auth.md`
- `WiKi/todo/phase-6/consulting 5/report from designer-reviewer.md`
- существующий код `crates/cli-harness`, `crates/agent-loop`, `crates/interactions`, `crates/auth`, `crates/harness-api`, `crates/conf`

# Дополнительные указания

1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`:
   - подробно описывай изменения в формате `было/стало/почему`;
   - если менялась архитектура приводи схемы (Mermaid) в формате `до/после/почему`;
   - если менялся установленный pipeline тоже приводи схемы (Mermaid) в формате `до/после/почему`;
4. Если необходимы действия со стороны пользователя пиши их в `manual.md` рядом с `todo.md`.
5. Если что-то непонятно, спроси!
