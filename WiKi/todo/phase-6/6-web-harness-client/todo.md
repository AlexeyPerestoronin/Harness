# Задача на разработку: Web-клиент `web-harness` (Leptos CSR)

Создать крейт `crates/web-harness`: чистое Rust/Leptos CSR-приложение, собираемое Trunk, работающее как standalone-клиент и как подключаемая библиотека (`HarnessPanel`/`HarnessGateway`) для других Leptos WASM-приложений.

Зачем: пользователь должен запускать и контролировать агентские сессии на удалённом/локальном `harness.exe --serve` из браузера; при этом клиент не знает про агентов, API-ключи и лог сессии.

Ожидаемый результат: `trunk build --release` в `crates/web-harness` собирает `dist`; standalone-клиент подключается к серверу, запускает/останавливает цикл, опрашивает `runUrl` каждые 10 секунд, показывает `pendingInput` и результат; библиотечная часть экспортирует `HarnessPanel` и `HarnessGateway`.

## Функциональные требования

### 1. Структура крейта

```
crates/web-harness/
  src/
    lib.rs            # экспорт HarnessGateway, HarnessClient, HarnessPanel
    main.rs           # тонкая standalone-точка входа: mount HarnessApp
    gateway.rs        # REST + polling
    state.rs          # HarnessClient: реактивное состояние
    components/
      harness_app.rs
      harness_panel.rs
      connect_dialog.rs
      status_bar.rs
      prompt_composer.rs
      input_modal.rs
      result_view.rs
      config_editor.rs   # только feature fs-panel
      fs_browser.rs      # только feature fs-panel
  Cargo.toml
  Trunk.toml
  index.html
```

1. `Cargo.toml` объявляет `[lib]` и `[[bin]]`; `version = "1.5.0"` (отдельная версия, крейт исключён из workspace — см. п.6).
2. Фичи: `default = []`, `fs-panel = []`. Без `fs-panel` компилируются только базовые компоненты; с `fs-panel` — дополнительно `ConfigEditor` и `FsBrowser`.
3. Стили: plain CSS с префиксом `hw-`; Tailwind не используется.

### 2. `HarnessGateway` (транспорт)

1. Сигнатура:
   ```rust
   pub struct HarnessGateway {
       base_url: String,
       auth_token: Option<String>,
   }
   impl HarnessGateway {
       pub fn new(base_url: impl Into<String>, auth_token: Option<String>) -> Self;
   }
   ```
2. Методы:
   - `health() -> Result<HealthResponse, GatewayError>`;
   - `start_run(prompt) -> Result<StartRunResponse, GatewayError>`;
   - `get_run_status(run_url) -> Result<RunStatusResponse, GatewayError>`;
   - `stop_run(reason) -> Result<..., GatewayError>`;
   - `submit_input(request_id, answer) -> Result<SubmitInputResponse, GatewayError>`;
   - с фичей `fs-panel`: методы Config API и FS API (`get_config`, `put_config`, `config_default`, `fs_list`, `fs_read`, `fs_write`, `fs_mkdir`, `fs_remove`, `fs_move`).
3. Все запросы — `fetch` через `gloo-net`; токен добавляется в заголовок `Authorization: Bearer <token>` для всех защищённых запросов.
4. `run_url` всегда относительный; `base_url` нормализуется (убрать trailing `/`), `run_url` присоединяется без повторного экранирования (корректировка §5.2 Designer-Reviewer).
5. Таймаут отдельного `fetch`-запроса — не более 10 секунд (корректировка §5.3 Designer-Reviewer); реализовать через гонку таймера и запроса.
6. SSE/EventSource/WebSocket не используются.

### 3. `HarnessClient` (реактивное состояние)

Хранит: адрес сервера, токен, `run_url`, текущий `RunStatusResponse`, флаг соединения, счётчик последовательных сетевых ошибок, признак открытого модального окна. Реализовать на сигналах Leptos.

### 4. Компоненты

1. `HarnessApp` — standalone-обёртка, монтирует `HarnessPanel`.
2. `HarnessPanel`:
   ```rust
   #[component]
   pub fn HarnessPanel(
       base_url: String,
       #[prop(optional)] auth_token: Option<String>,
       #[prop(optional)] compact: bool,
       #[prop(optional)] theme: Option<HarnessTheme>,
   ) -> impl IntoView
   ```
   Без `fs-panel` — только цикл (промпт, состояние, модальные окна, результат); с `fs-panel` — вкладки «Конфигурация» и «Файлы».
3. `ConnectDialog`:
   - запрашивает адрес сервера (можно из query-параметра `?server=...`) и токен;
   - `GET /health` без токена → при `authRequired: true` показывает поле токена;
   - после ввода токена проверяет `GET /api/v1/run/current`;
   - `401` → сброс сохранённого токена и повторный диалог с сообщением «неверный токен»;
   - токен хранится только в `localStorage`/`sessionStorage` текущего origin; через query-параметры не передаётся.
4. `StatusBar`:
   - показывает `runState`, номер итерации, токены, USD (приблизительно и real), время с момента последнего ответа;
   - лог сессии не отображается.
5. `PromptComposer`:
   - textarea промпта, кнопки Start/Stop;
   - Start → `POST /run`, сохраняет `runUrl`;
   - Stop → `POST /run/stop`;
   - во время `running`/`waiting_input` Start недоступна, Stop доступна; в `idle` — наоборот; в `stopping` — Start недоступна.
6. Polling:
   - после получения `runUrl` опрашивать `GET {runUrl}` каждые 10 секунд;
   - при `pendingInput` — открывать `InputModal`;
   - при `result` — показывать `ResultView` и останавливать polling;
   - при сетевой ошибке сохранять последнее известное состояние, показывать «нет соединения» и продолжать опрос;
   - после 6 последовательных неудач показывать модальное окно «Сервер не найден» с инструкцией и кнопкой «Повторить подключение».
7. `InputModal`:
   - показывает `prompt` из `pendingInput`;
   - `kind = "confirm"` → кнопки Yes/No;
   - `kind = "text"` → textarea/input;
   - `sensitive == true` → поле ввода типа password (корректировка H1 Designer-Reviewer);
   - эвристика по подстроке `"API-ключ"`/`"api-key"`/`"API key"` НЕ используется как основной механизм;
   - отправка — `POST /run/input { requestId, answer }`; после отправки модальное окно закрывается;
   - любой новый `pendingInput` обрабатывается одинаково (включая двухшаговый Deny → причина).
8. `ResultView`:
   - `finished`/`failed`/`stopped` + статистика (итерации, токены, USD) + текст ошибки из `result.error` при `ok = false`;
   - кнопка «Новый запуск» очищает результат и активирует Start.
9. `ConfigEditor` (только `fs-panel`):
   - `GET /config` → textarea с моноширинным шрифтом;
   - «Сохранить» → `PUT /config`; при `400 CONFIG_INVALID` показывать сообщение и не давать запускать цикл, пока конфигурация не исправлена;
   - при отсутствии файла — «Создать по умолчанию» (`POST /config/default`);
   - секретные поля показываются как `"***"`; сохранение `"***"` не затирает реальное значение.
10. `FsBrowser` (только `fs-panel`):
    - работает через FS API: список, чтение, создание/изменение файла, создание каталога, удаление, перемещение/переименование;
    - подтверждение перед удалением/перезаписью;
    - защищённые deny-list'ом пути не отображаются либо помечаются как недоступные (сервер возвращает `403`).
11. Модальное окно «Сервер не найден / CORS запрещён» — с инструкцией по `ui.md` §3.2 (скачать/запустить `harness.exe`, принять самоподписанный сертификат, проверить `server.cors-allowed-origins`), кнопками «Повторить подключение» и «Скопировать команду запуска».

### 5. Адаптивность и доступность

1. Полный режим — двухколоночная раскладка; при `fs-panel` — вкладки.
2. Компактный режим (`compact = true`) — ширина ~320–420 px, одна колонка, модальные окна поверх панели.
3. Доступность по `ui.md` §8: текстовые подписи, цвет не единственный индикатор, `Escape` закрывает модальные окна (с подтверждением при несохранённом ответе), понятные ошибки TLS/CORS/mixed-content.

### 6. Workspace

Крейт `web-harness` не должен собираться нативным workspace-билдом (он wasm-only). Обновить корневой `Cargo.toml`: добавить `exclude = ["crates/web-harness"]` в секцию `[workspace]` (или эквивалентно явным списком `members`). Сам `crates/web-harness/Cargo.toml` должен быть самодостаточным: без `workspace = true`-наследования и с собственной секцией `[workspace]` (пустой), чтобы Cargo корректно собирал его как standalone-крейт из каталога `crates/web-harness`.

## Проверка корректности решения

1. `cd crates/web-harness && trunk build --release` — успех, создан `dist`.
2. `cargo build --workspace` нативно — успех (web-harness исключён).
3. Юнит-тесты чистой логики (насколько возможно в WASM):
   - `HarnessGateway::join_url`/нормализация `base_url`+`runUrl`;
   - разбор `RunStatusResponse` с `pendingInput` (`sensitive: false` по умолчанию и `true` при наличии);
   - переходы состояний polling-цикла (`idle` → `running` → `waiting_input` → `finished`).
4. Ручная проверка standalone:
   - открыть `index.html` (через `trunk serve` или GitHub Pages), подключиться к локальному серверу, ввести токен;
   - запустить цикл, дождаться статуса, при появлении `pendingInput` ответить; остановить цикл; увидеть результат;
   - проверить password-режим при `sensitive: true` и отсутствие эвристики по подстроке.

## Scope

Минимально необходимые изменения:

- `crates/web-harness/**` (новый крейт)
- `Cargo.toml` (исключение `crates/web-harness` из workspace)

Справочные материалы для чтения:

- `WiKi/todo/phase-6/consulting 5/base.md`
- `WiKi/todo/phase-6/consulting 5/arch.md`
- `WiKi/todo/phase-6/consulting 5/tech.md`
- `WiKi/todo/phase-6/consulting 5/api.md`
- `WiKi/todo/phase-6/consulting 5/auth.md`
- `WiKi/todo/phase-6/consulting 5/ui.md`
- `WiKi/todo/phase-6/consulting 5/report from designer-reviewer.md`
- существующий код `crates/harness-api`

# Дополнительные указания

1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
