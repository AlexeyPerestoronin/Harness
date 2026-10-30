# Phase 6. Архитектура решения

> Документ описывает целевую архитектуру: декомпозицию на crate'ы, модули, связи, диаграммы и модель выполнения.
> Смежные документы: `base.md` (исходное/целевое состояние), `tech.md` (технологии), `api.md` (REST-API), `ui.md` (web-интерфейс), `auth.md` (аутентификация).

---

## 1. Архитектурные принципы

1. **Единая зона ответственности ввода/вывода.** Все операции ввода/вывода и логирования проходят через два контракта: `Logger` и `Interaction`.
2. **Полностью асинхронный сервер.** HTTP-слой, агентский цикл и запросы к вендорам работают на `tokio`; блокирующие операции изолируются через `spawn_blocking`.
3. **Сервер ничего не знает о клиенте.** Сервер предоставляет HTTPS REST + SSE; web-интерфейс — отдельное приложение.
4. **Одна активная сессия.** Сервер в каждый момент выполняет не более одного агентского цикла.
5. **Безопасность по умолчанию.** HTTPS обязателен, CORS по умолчанию запрещён, все API-эндпоинты (кроме `health`) требуют Bearer-токен.
6. **Секреты не покидают сервер.** API-ключи и приватные ключи никогда не возвращаются клиенту; в файловых ответах выполняется redaction.
7. **Модульность для встраивания.** Клиент разделён на транспорт (`HarnessGateway`) и UI (`HarnessPanel`); FS-возможности включаются отдельной фичей.
8. **YAGNI.** Runtime-плагины, WIT-компоненты, сложная многофакторная аутентификация и удалённый запуск сервера исключены из первой версии.

---

## 2. Декомпозиция на crate'ы

### 2.1. Целевая структура

```
crates/
  conf/           # существующий: загрузка конфигурации, модели, список агентов, ServerSettings
  logger/         # Logger, FileLogger, NullLogger
  interactions/   # Interaction, OutputType, CliInteraction, композиция приёмников
  auth/           # генерация и проверка Bearer-токена, axum-экстрактор
  harness-api/    # общие DTO REST/SSE (сервер и клиент)
  tools/          # реестр инструментов; использует Interaction вместо stdin/logger
  agent-loop/     # асинхронный агентский цикл, агенты, VendorRequest, RequestPipeline
  cli-harness/    # CLI + режим HTTPS-сервера (axum); WebLogger и WebInteraction
  web-harness/    # Leptos CSR клиент + HarnessGateway + HarnessPanel (lib.rs + main.rs)
```

### 2.2. Ответственность crate'ов

| Crate | Тип | Ответственность |
|---|---|---|
| `logger` | lib | Контракт `Logger`, файловый и пустой приёмники |
| `interactions` | lib | Контракт `Interaction`, `OutputType`, `CliInteraction`, `InteractionLoggers` |
| `auth` | lib | Генерация токена, constant-time сравнение, `AuthToken`, `AuthError`, axum-экстрактор |
| `harness-api` | lib | DTO для REST и SSE: запросы, ответы, события |
| `conf` | lib | Загрузка/валидация `harness.json5` и моделей; публичный список агентов; `ServerSettings` |
| `tools` | lib | Реестр инструментов агента; `UserModule` и логирование работают через `Interaction` |
| `agent-loop` | lib | Асинхронный `AgentLoop`, вендорные агенты, `VendorRequest`, `RequestPipeline` |
| `cli-harness` | bin | CLI-режим (`--prompt`) и серверный режим (`--serve`/без параметров); TLS, `WebLogger`, `WebInteraction` |
| `web-harness` | lib + bin | Leptos-клиент, `HarnessGateway`, RSA-модуль, `HarnessPanel`; тонкий `main.rs` для standalone |

### 2.3. Граф зависимостей

В терминах Cargo (стрелка — «зависит от»):

```mermaid
graph LR
    logger[logger]
    harness-api[harness-api]
    auth[auth]
    interactions[interactions] --> logger
    interactions --> tokio
    conf[conf]
    tools[tools] --> conf
    tools --> interactions
    agent-loop[agent-loop] --> conf
    agent-loop --> tools
    agent-loop --> interactions
    cli-harness[cli-harness] --> agent-loop
    cli-harness --> conf
    cli-harness --> interactions
    cli-harness --> harness-api
    cli-harness --> logger
    cli-harness --> auth
    cli-harness --> axum
    cli-harness --> tokio
    web-harness[web-harness] --> harness-api
    web-harness --> leptos
```

Пояснения:

- `interactions` **использует** `logger`, поэтому в Cargo-графе `interactions → logger`; обратная зависимость создала бы цикл.
- `tools`, `agent-loop`, `cli-harness` зависят от `interactions` и не зависят от `logger` напрямую.
- `auth` — тонкий crate без бизнес-зависимостей; он используется только `cli-harness` для защиты HTTP-слоя.
- `harness-api` не зависит от axum/leptos/auth, поэтому подключается и к серверу, и к WASM-клиенту.
- `web-harness` не реализует `Interaction`; он потребляет `harness-api` и общается с сервером по HTTPS REST/SSE.

---

## 3. Контекстная диаграмма (C4 Context)

```mermaid
graph LR
    U[Пользователь<br/>браузер]
    W[harness.wasm<br/>Leptos CSR<br/>GitHub Pages или host-приложение]
    S[harness.exe --serve<br/>axum: HTTPS REST + SSE<br/>Bearer auth]
    FS[Файловая система сервера<br/>cwd, harness.json5, роли, навыки, агенты, логи]
    V[Вендорные LLM API<br/>xAI/Google/Claude/DeepSeek/AliBaba]

    U -->|HTTPS| W
    W -->|HTTPS REST + SSE + Bearer| S
    S -->|async HTTPS| V
    S -->|чтение/запись| FS
```

Сервер не раздаёт frontend. `harness.wasm` доставляется пользователю либо с GitHub Pages, либо в составе родительского Leptos-приложения.

---

## 4. Компонентная диаграмма

```mermaid
graph TB
    subgraph Server[cli-harness: harness.exe --serve]
        CLI[CLI: --serve, --port, --host, --conf, --cwd, --auth-token]
        TLS[TLS: rustls / rcgen]
        AX[axum Router]
        AUTH[auth extractor: Bearer token]
        ST[ServerState]
        WI[WebInteraction]
        WL[WebLogger]
        EM[EventBus: кольцевой буфер + broadcast]
        FSAPI[FS REST API + redaction]
        CRYPTO[RSA endpoints]
        CONFAPI[Config/Agents API]
    end

    subgraph AgentLoopCrate[agent-loop]
        AL[AgentLoop async]
        AG[Async agents]
        VR[VendorRequest async reqwest]
        PL[RequestPipeline pre/dur/post]
        TR[ToolRegistry]
    end

    subgraph Client[web-harness: Leptos CSR]
        APP[HarnessApp]
        GW[HarnessGateway]
        KEY[RSA key module]
        PANEL[HarnessPanel]
        VIEWS[Composer / Log / InputModal / ApiKeyModal / ConnectDialog]
        FSVIEWS[ConfigEditor / FsBrowser — только feature fs-panel]
    end

    APP --> GW
    APP --> PANEL
    PANEL --> VIEWS
    PANEL -.->|feature fs-panel| FSVIEWS
    KEY --> GW
    GW -->|HTTPS REST + SSE| TLS
    TLS --> AX
    AX --> AUTH
    AUTH --> ST
    ST --> AL
    AL --> AG
    AG --> VR
    VR --> PL
    AL --> TR
    TR --> WI
    WI --> EM
    WL --> EM
    ST --> FSAPI
    ST --> CRYPTO
    ST --> CONFAPI
```

---

## 5. Единая зона ответственности ввода/вывода

### 5.1. Контракт `Logger`

Трейт `Logger` упрощается до одного метода:

```rust
/// Контракт журнала сессии.
pub trait Logger: Send + Sync {
    /// Записать фрагмент `message`.
    ///
    /// `new_line = true`  — запись начинается с новой строки:
    ///                      если `message` не начинается с '\n', он добавляется.
    /// `new_line = false` — запись продолжает текущую позицию (как есть).
    fn log_str(&self, message: &str, new_line: bool);
}
```

Реализации:

| Реализация | Где живёт | Поведение |
|---|---|---|
| `FileLogger` | `crates/logger` | Дописывает данные в файл лога сессии |
| `NullLogger` | `crates/logger` | Пустой приёмник для тестов |
| `WebLogger` | `crates/cli-harness/src/server` | Реализует `Logger` и отправляет данные клиенту через SSE |

`WebLogger` находится в серверном слое `cli-harness`, потому что зависит от серверной событийной шины.

### 5.2. Контракт `Interaction`

Контракт ввода/вывода живёт в crate `interactions`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    /// Продолжает запись с текущей позиции.
    ContinueLine,
    /// Начинает запись с новой строки.
    NewLine,
    /// Перезаписывает текущую строку (консоль/индикатор прогресса).
    RewriteLine,
}

#[derive(Debug)]
pub enum InteractionError {
    Cancelled,
    Timeout,
    Io(String),
}

#[async_trait::async_trait]
pub trait Interaction: Send + Sync {
    /// Запросить у пользователя текстовый ввод.
    async fn input(
        &self,
        prompt: &str,
        write_log: bool,
    ) -> Result<String, InteractionError>;

    /// Вывести пользователю информацию.
    async fn output(
        &self,
        message: &str,
        write_log: bool,
        new_line: OutputType,
    ) -> Result<(), InteractionError>;
}
```

**Обоснование сигнатуры.** Метод `input` обязан передавать агенту текст запроса и возвращать ответ пользователя, поэтому принимает `prompt` и возвращает `Result<String, InteractionError>`. Методы объявлены `async`, так как в серверном режиме ожидание ответа пользователя может длиться минутами и не должно блокировать поток `tokio`; CLI-реализация выносит чтение `stdin` в `spawn_blocking`.

### 5.3. Композиция приёмников

```rust
pub struct InteractionLoggers {
    /// Файловый приёмник. Пишется только при write_log = true.
    pub file: Option<Arc<dyn Logger>>,
    /// Прогресс-приёмники (SSE/клиент). Пишутся всегда.
    pub progress: Vec<Arc<dyn Logger>>,
}
```

| Реализация | Где живёт | Состав приёмников |
|---|---|---|
| `CliInteraction` | `crates/interactions` | `file = Some(FileLogger)`, `progress = []` |
| `WebInteraction` | `crates/cli-harness/src/server` | `file = Some(FileLogger)`, `progress = [WebLogger]` |

Правила логирования ввода/вывода:

| Флаг | Файловый приёмник | Прогресс-приёмники (SSE) |
|---|---|---|
| `write_log = true` | пишется | пишутся |
| `write_log = false` | **не пишется** | **пишутся** (таймеры, служебный прогресс) |

Текст вопроса из `Interaction::input` **всегда** дополнительно отправляется в SSE-лог (событие `output`, режим `new_line`), даже если `write_log = false`. Это гарантирует, что вопрос виден не только в модальном окне, но и в общем журнале.

### 5.4. Отображение `OutputType` на приёмники

| OutputType | CLI (stdout) | FileLogger | Web/SSE |
|---|---|---|---|
| `ContinueLine` | `print!("{message}")` | `log_str(message, false)` | событие `output`, `mode: "continue_line"` |
| `NewLine` | запись с новой строки | `log_str(message, true)` | событие `output`, `mode: "new_line"` |
| `RewriteLine` | `\r{message}` (перезапись строки) | `log_str(message, false)` | событие `output`, `mode: "rewrite_line"`; UI заменяет последнюю строку |

---

## 6. Сетевой слой агента: `RequestPipeline`

### 6.1. Назначение

`RequestPipeline` — контракт сетевого запроса к вендору, выносящий повторяющуюся логику (pre-логирование, периодические проверки во время ожидания, post-логирование) из конкретных HTTP-агентов.

### 6.2. Улучшенная модель

Базовый эскиз со списками `Arc<dyn Fn()>` недостаточен, потому что:

- действия должны быть асинхронными (`Interaction::output` асинхронный);
- действиям нужен контекст (номер попытки, URL, признак отмены);
- обработку ошибок нельзя сводить к `panic!`/игнорированию;
- чтение тела ответа тоже должно быть отменяемым.

Поэтому вводится контракт с тремя типами действий и контекстом:

```rust
#[derive(Debug, Clone)]
pub struct PipelineContext {
    pub attempt: usize,
    pub url: String,
    pub started_at: std::time::Instant,
    pub tick: Duration,
    pub cancel: CancellationToken,
}

#[derive(Debug)]
pub enum PipelineError {
    Cancelled,
    Transport(String),
    Hook(String),
}

#[async_trait::async_trait]
pub trait PreAction: Send + Sync {
    async fn execute(&self, ctx: &PipelineContext) -> Result<(), PipelineError>;
}

#[async_trait::async_trait]
pub trait DurAction: Send + Sync {
    async fn execute(&self, ctx: &PipelineContext) -> Result<(), PipelineError>;
}

#[async_trait::async_trait]
pub trait PostAction: Send + Sync {
    async fn execute(
        &self,
        ctx: &PipelineContext,
        response: &Response,
    ) -> Result<(), PipelineError>;
}

#[async_trait::async_trait]
pub trait RequestPipeline<T>: Send + Sync
where
    T: Serialize + Send + Sync,
{
    fn build(&self, client: &Client, payload: &T) -> RequestBuilder;

    fn pre_actions(&self)  -> &[Arc<dyn PreAction>];
    fn dur_actions(&self)  -> &[Arc<dyn DurAction>];
    fn post_actions(&self) -> &[Arc<dyn PostAction>];

    fn add_pre(&mut self, action: Arc<dyn PreAction>);
    fn add_dur(&mut self, action: Arc<dyn DurAction>);
    fn add_post(&mut self, action: Arc<dyn PostAction>);

    async fn execute(
        &self,
        client: &Client,
        payload: &T,
        ctx: PipelineContext,
    ) -> Result<Response, PipelineError> {
        for action in self.pre_actions() {
            action.execute(&ctx).await?;
        }

        let request = self.build(client, payload).send();
        tokio::pin!(request);

        let mut ticker = tokio::time::interval(ctx.tick);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let response = loop {
            tokio::select! {
                _ = ctx.cancel.cancelled() => {
                    return Err(PipelineError::Cancelled);
                }
                _ = ticker.tick() => {
                    for action in self.dur_actions() {
                        action.execute(&ctx).await?;
                    }
                }
                result = &mut request => {
                    break result.map_err(|e| PipelineError::Transport(e.to_string()))?;
                }
            }
        };

        for action in self.post_actions() {
            action.execute(&ctx, &response).await?;
        }

        Ok(response)
    }
}
```

Конкретная JSON-реализация (`JsonPipeline<T>`) хранит URL, дополнительные заголовки и списки действий; retry/rate-limit остаются в `VendorRequest` и используют `RequestPipeline::execute` для одной попытки.

### 6.3. Чтение тела ответа с отменой

`VendorRequest` оборачивает не только отправку, но и чтение тела ответа в `tokio::select!` с `CancellationToken`. Для потоковых/«думающих» моделей длительное чтение тела прерывается так же, как и ожидание статуса:

```rust
let read_body = async { response.text().await };

tokio::select! {
    _ = cancel.cancelled() => {
        return Err("request cancelled".into());
    }
    text = read_body => {
        // классификация статуса, retry и т.д.
    }
}
```

Во время ожидания ответа `DurAction`-действия выполняются циклически раз в секунду и проверяют:

- признак отмены;
- актуальность rate-limit/секундомера;
- необходимость отправить прогресс-событие клиенту.

---

## 7. Полностью асинхронный агентский цикл

### 7.1. Что меняется

| Компонент | Текущее состояние | Целевое состояние |
|---|---|---|
| `VendorRequest` | `reqwest::blocking`, `thread::sleep`, `mpsc` + поток для stopwatch | `reqwest` async, `tokio::time::sleep`, `RequestPipeline` + `tokio::select!` |
| `Agent` | синхронные методы | `async fn` методы |
| `AgentLoop::start` | блокирующий цикл | `async fn` с `CancellationToken` |
| `ToolRegistry::call` | синхронный | `async fn`; блокирующие модули (`fs`, `git`, `terminal`) через `spawn_blocking` |
| Ввод/вывод | stdin/stdout напрямую | `Interaction` (async) |
| Логирование в `tools`/`agent-loop` | `Arc<dyn Logger>` напрямую | `Arc<dyn Interaction>`; логи через `Interaction::output` |

### 7.2. Отмена

- В `ServerState` и в `ActiveRun` хранится `tokio_util::sync::CancellationToken`.
- `AgentLoop` перед каждой итерацией и перед каждым вендорным запросом проверяет отмену.
- Активный HTTP-запрос и чтение тела ответа оборачиваются в `tokio::select!` с отменой.
- `WebInteraction::input` при отмене завершает ожидающий `oneshot` с ошибкой `Cancelled`.

---

## 8. Серверная часть

### 8.1. CLI-режимы

```text
harness.exe                                # HTTPS-сервер, порт по умолчанию
harness.exe --serve --port 4067            # HTTPS-сервер, явный порт
harness.exe --serve --host 0.0.0.0         # удалённый доступ
harness.exe --serve --tls-cert cert.pem --tls-key key.pem
harness.exe --serve --auth-token <token>   # фиксированный Bearer-токен
harness.exe --prompt "..." --conf ...      # консольный режим (как раньше)
harness.exe --dump ...                     # возобновление из dump (консольный режим)
```

Если `--cwd` не задан, используется текущая рабочая директория процесса; `--conf` по умолчанию указывает на `harness.json5` внутри `cwd`.

Приоритет порта:

1. `--port <n>` из CLI;
2. `server.port` из `harness.json5`;
3. встроенное значение по умолчанию — `4067`.

Если `--tls-cert`/`--tls-key` не заданы, сервер автоматически генерирует самоподписанный сертификат и сохраняет его в `cwd/.harness/certs/`, чтобы браузер можно было однократно пометить как доверенный.

### 8.2. `ServerState`

Все блокировки — только `tokio::sync`, чтобы guard'ы можно было удерживать через `.await`:

```rust
struct ServerState {
    config: tokio::sync::RwLock<LoadedServerConfig>,
    active: tokio::sync::Mutex<Option<ActiveRun>>,
    events_tx: broadcast::Sender<(u64, ServerEvent)>,
    event_log: tokio::sync::Mutex<EventRingBuffer>,
    rsa_private: RsaPrivateKey,
    encrypted_keys: tokio::sync::Mutex<HashMap<String, Vec<u8>>>,
    auth_token: AuthToken,
    live_timeout: Duration,
}
```

`EventRingBuffer` — кольцевой буфер `VecDeque<(u64, ServerEvent)>` ёмкостью `server.sse-buffer-size` (по умолчанию 512).

`LoadedServerConfig` хранит:

- путь к `harness.json5`;
- `config_state`: `ok` | `missing` | `invalid`;
- загруженные `Settings`, если конфигурация валидна;
- параметры `server` (host, port, live-timeout, interaction-timeout, sse-buffer-size, cors-allowed-origins, tls).

### 8.3. Модель выполнения (одна активная сессия)

```mermaid
stateDiagram-v2
    [*] --> Idle: сервер запущен
    Idle --> Running: POST /run
    Running --> WaitingInput: Interaction/input
    WaitingInput --> Running: POST /run/input
    WaitingInput --> Failed: interaction-timeout истёк
    Running --> Stopping: POST /run/stop
    WaitingInput --> Stopping: POST /run/stop
    Stopping --> Stopped: цикл прерван
    Running --> Finished: агент вернул stop
    Running --> Failed: ошибка
    Finished --> Idle
    Failed --> Idle
    Stopped --> Idle
    Idle --> [*]: live-timeout истёк
```

Правила:

- Повторный `POST /run` при активном цикле возвращает `409`.
- После завершения цикла сервер возвращается в `Idle`, и таймер `live-timeout` перезапускается.
- Polling-запросы (`GET /health`, `GET /agents`, `GET /config`) **не** продлевают `live-timeout`.
- `server.interaction-timeout` по умолчанию — 10 минут. По истечении `WebInteraction::input` возвращает `InteractionError::Timeout`, агентский цикл завершается с ошибкой, `runState` становится `failed`.

### 8.4. Запуск сервера без конфигурации

Сервер должен запускаться, даже если `harness.json5` отсутствует или повреждён, чтобы локальный пользователь мог настроить среду вручную:

- `config_state = "missing"` — файла нет, `POST /run` возвращает `412` до появления валидной конфигурации;
- `config_state = "invalid"` — файл есть, но не прошёл валидацию; `POST /run` возвращает `412`;
- `config_state = "ok"` — конфигурация загружена, запуск возможен.

Standalone web-клиент в этих состояниях показывает инструкцию по локальной настройке сервера, а не пытается создать/исправить файлы удалённо.

### 8.5. Аутентификация и CORS

- `GET /health` — открытый; возвращает `authRequired: true`, состояние сервера и цикла.
- Все остальные `/api/v1/*` требуют заголовок `Authorization: Bearer <token>`.
- Токен генерируется при старте сервера (32 случайных байта, base64url) и печатается в stdout; при необходимости его можно зафиксировать через `--auth-token`.
- Проверка токена — constant-time сравнение, реализовано в crate `auth`.
- CORS по умолчанию запрещён: сервер не отправляет `Access-Control-Allow-Origin`.
- Если в конфигурации задан `server.cors-allowed-origins`, сервер отправляет CORS-заголовки только для точного совпадения `Origin` из этого списка. Значение `"*"` в первой версии не поддерживается.

Подробнее — `auth.md`.

---

## 9. Потоки данных

### 9.1. Запуск цикла

```mermaid
sequenceDiagram
    participant B as Browser (Leptos)
    participant S as harness.exe (HTTPS axum)
    participant AL as AgentLoop (async)
    participant V as Vendor LLM

    B->>S: GET /api/v1/health (без токена)
    S-->>B: ok + runState + configState + authRequired
    B->>S: GET /api/v1/agents (Bearer)
    S-->>B: список агентов (без секретов)
    alt агент требует API-ключ
        B->>S: GET /api/v1/crypto/public-key (Bearer)
        S-->>B: RSA public key
        B->>B: шифрует ключ RSA-OAEP-256
        B->>S: POST /api/v1/crypto/api-key (Bearer)
        S-->>B: accepted
    end
    B->>S: POST /api/v1/run {prompt, agentIndex} (Bearer)
    S-->>B: 202 {runState: running}
    par параллельно
        S->>AL: start async (сразу после POST /run)
    and
        B->>S: GET /api/v1/run/events (SSE, Bearer)
        S-->>B: replay из кольцевого буфера + live-события
    end
    AL->>V: async model request
    V-->>AL: response / tool calls
    AL->>S: Interaction::output → WebLogger → EventBus
    S-->>B: SSE event: output / status
```

### 9.2. Вопрос пользователю

```mermaid
sequenceDiagram
    participant AL as AgentLoop
    participant WI as WebInteraction
    participant EM as EventBus
    participant S as Axum
    participant B as Browser

    AL->>WI: input(prompt, write_log)
    WI->>EM: ServerEvent::Output (текст вопроса)
    WI->>EM: ServerEvent::InputRequest
    EM->>S: broadcast + кольцевой буфер
    S-->>B: SSE output + input_request
    WI->>WI: ожидание oneshot с таймаутом interaction-timeout
    B->>S: POST /run/input {requestId, answer}
    S->>WI: UserResponse::Answer
    WI-->>AL: Ok(answer)
```

По истечении `interaction-timeout`:

```mermaid
sequenceDiagram
    participant WI as WebInteraction
    participant AL as AgentLoop
    participant S as Axum
    participant B as Browser

    WI->>WI: tokio::time::timeout истёк
    WI-->>AL: Err(InteractionError::Timeout)
    AL->>AL: завершить цикл с ошибкой
    AL->>S: ServerEvent::Result {runState: failed}
    S-->>B: SSE result
```

### 9.3. Остановка

```mermaid
sequenceDiagram
    participant B as Browser
    participant S as Axum
    participant ST as ServerState
    participant AL as AgentLoop

    B->>S: POST /run/stop (Bearer)
    S->>ST: active.cancel()
    ST->>AL: CancellationToken отменён
    AL->>AL: аборт HTTP-запроса / чтения тела / input
    S-->>B: 202 {runState: stopping}
    AL-->>S: результат: stopped
    S-->>B: SSE result
```

### 9.4. Переподключение SSE

```mermaid
sequenceDiagram
    participant B as Browser
    participant S as Axum
    participant RB as EventRingBuffer
    participant BC as broadcast

    B->>S: GET /api/v1/run/events (Last-Event-ID: 42)
    S->>RB: выбрать события с id > 42
    RB-->>S: VecDeque<(id, ServerEvent)>
    S-->>B: replay-события (с .id(id))
    S->>BC: subscribe()
    loop live
        BC-->>S: ServerEvent
        S-->>B: SSE event (с .id(id))
    end
```

---

## 10. Передача API-ключа

### 10.1. Схема

```mermaid
sequenceDiagram
    participant B as web-harness
    participant S as harness.exe
    participant A as AgentLoop

    B->>S: GET /api/v1/agents (Bearer)
    S-->>B: agents[] + apiKeyRequired
    alt apiKeyRequired == true
        B->>S: GET /api/v1/crypto/public-key (Bearer)
        S-->>B: RSA public key (SPKI/PEM)
        B->>B: проверяет длину ключа ≤ 128 байт и шифрует RSA-OAEP-256
        B->>S: POST /api/v1/crypto/api-key {agentIndex, encryptedApiKey} (Bearer)
        S->>S: расшифровывает, проверяет UTF-8 и длину ≤ 128 байт
        S-->>B: 202 accepted
    end
    B->>S: POST /api/v1/run {prompt, agentIndex} (Bearer)
    S->>A: запуск с расшифрованным API-ключом
```

### 10.2. Правила хранения и ограничения

- Сервер хранит **зашифрованный** API-ключ для конкретного вендора до остановки сервера.
- Расшифрованный API-ключ появляется только в момент запуска агентского цикла и живёт только в памяти этого цикла.
- Приватный RSA-ключ хранится только в памяти сервера.
- **Максимальная длина открытого API-ключа — 128 байт.** Сервер возвращает `400`, если после расшифрования длина больше 128 байт; клиент проверяет длину до шифрования.
- Если модель уже содержит ключ в конфигурации, RSA-обмен не требуется.

---

## 11. Файловое API и защита секретов

FS API сохраняется на сервере, но используется только встраиваемыми web-приложениями (плагинами). Standalone-клиент его не вызывает.

Правила redaction:

1. `GET /api/v1/fs/read` и `GET /api/v1/config` **никогда** не возвращают значения секретных полей (`api-key`, `apikey`, `token`, `secret`, `private-key`). Вместо значения возвращается `"***"`.
2. `PUT /api/v1/fs/write` и `PUT /api/v1/config` поддерживают merge-правило: если во входном тексте значение секретного поля равно `"***"` (или `"<redacted>"`), сервер сохраняет прежнее значение из существующего файла, а не затирает его.
3. Новое фактическое значение секретного поля через FS/Config API записать можно; обратно оно никогда не возвращается. Интерактивный ввод API-ключа в standalone-клиенте выполняется через `POST /api/v1/crypto/api-key`.
4. Пути по-прежнему ограничены `cwd`; запрещены `..`, абсолютные пути и symlink-выходы за пределы `cwd`.

---

## 12. Встраивание в другие Leptos-приложения

### 12.1. Модель интеграции

Используется **source-level интеграция**: родительское приложение подключает `web-harness` как обычный Cargo-крейт через Git.

```toml
[dependencies]
web-harness = { git = "https://github.com/<org>/Harness.git", tag = "v1.5.0", features = ["fs-panel"] }
```

`trunk build` родительского приложения компилирует `web-harness` непосредственно в его WASM-бандл.

### 12.2. Отображение UI модуля

`web-harness` экспортирует компоненту:

```rust
use web_harness::HarnessPanel;

#[component]
pub fn HostApp() -> impl IntoView {
    view! {
        <main>
            <h1>"Моя IDE"</h1>
            <aside class="side-panel">
                <HarnessPanel
                    base_url="https://localhost:4067".to_string()
                    compact=true
                />
            </aside>
        </main>
    }
}
```

- Без фичи `fs-panel` `HarnessPanel` показывает только цикл: агент, промпт, лог, модальные окна.
- С фичей `fs-panel` дополнительно доступны вкладки «Конфигурация» и «Файлы», работающие через FS API.
- Токен аутентификации родительское приложение может передать через prop `auth_token` либо через `HarnessGateway`.

### 12.3. Программное взаимодействие

```rust
let gateway = HarnessGateway::new("https://localhost:4067", auth_token);
let agents = gateway.list_agents().await?;
gateway.start_run(StartRunRequest { prompt, agent_index }).await?;
gateway.stop_run().await?;
gateway.send_input(request_id, answer).await?;
gateway.events().await?; // Stream<Item = ServerEvent>
```

---

## 13. Риски и ограничения

| Риск | Влияние | Митигация |
|---|---|---|
| Самоподписанный HTTPS-сертификат требует ручного доверия в браузере | Среднее | Автоматическая генерация сертификата, инструкция в UI, возможность подставить собственный сертификат |
| Миграция агентов на async может внести регрессии в retry/rate-limit | Высокое | Сохранить логику `VendorRequest`, заменив только транспортный слой; покрыть тестами |
| Блокирующие инструменты в async-цикле | Среднее | Всегда вызывать `fs`/`git`/`terminal` через `spawn_blocking` |
| Утечка секретов через FS/Config API | Критичное | Redaction на чтении, merge-правило на записи, тесты на отсутствие `api-key` в ответах |
| Cross-site-доступ к локальному серверу | Критичное | CORS запрещён по умолчанию + Bearer-токен + bind `127.0.0.1` по умолчанию |
| Рост логов при длинных циклах | Низкое | Кольцевой буфер SSE, `tail`/`offset` у log API |
| Web Crypto требует secure context | Низкое | Сервер работает по HTTPS; `https://localhost` считается secure context |
| Конкурентный доступ к файлам из UI и агентского цикла | Среднее | Во время активного цикла UI показывает предупреждение; standalone-клиент FS не использует |
