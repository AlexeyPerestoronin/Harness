# Результат: создание `REST-API.help.md`

## Итог

Задача выполнена: в корне репозитория создан самодостаточный справочный файл
`REST-API.help.md` на английском языке. Документ покрывает весь реализованный
REST API серверного режима `harness.exe --serve` и содержит как
человекочитаемые описания с примерами, так и встроенный OpenAPI 3.0 (YAML)
блок для машинной обработки/импорта.

## Было

- Единого справочного файла по REST API не существовало.
- Описание API было распределено между `WiKi/todo/phase-6/consulting 5/api.md`
  и исходным кодом обработчиков (`crates/cli-harness/src/server/*.rs`,
  `crates/harness-api/src/lib.rs`).

## Стало

Создан `REST-API.help.md`, содержащий:

1. **General information** — base URL, HTTPS-only, Bearer-аутентификация,
   формат ошибки `{ "error": { "code", "message" } }`, CORS-правила, сводная
   таблица HTTP-статусов.
2. **Health** — `GET /api/v1/health`, назначение, пример ответа, таблица полей.
3. **Run lifecycle** — `POST /run`, `GET /run/current`, `POST /run/stop`,
   `POST /run/input`: запросы, ответы, ошибки, состояния, поля `pendingInput`
   и `result`.
4. **Config API** — `GET /config`, `PUT /config`, `POST /config/default`;
   правила redaction секретов и merge-правило.
5. **FS API** — все 6 эндпоинтов, правила путей, deny-list, redaction/merge,
   примеры запросов/ответов и ошибок.
6. **Client polling scenario** — пошаговый сценарий `POST /run` →
   `GET {runUrl}` каждые 10 секунд → `pendingInput` → `POST /run/input` →
   `result`.
7. **Endpoint summary** — сводная таблица из 14 эндпоинтов с признаком auth.
8. **OpenAPI 3.0 notation** — встроенный YAML-блок со всеми эндпоинтами,
   securityScheme, компонентами схем и типовыми ответами.

## Почему так

- Документ размещён в корне репозитория, как требует задача, чтобы
  пользователь/интегратор мог найти его без чтения исходного кода.
- Язык документа — английский, без русскоязычных фрагментов.
- Контент сверен с фактическим роутером из
  `crates/cli-harness/src/server/handlers.rs` и DTO из
  `crates/harness-api/src/lib.rs`, поэтому в документе используются реальные
  camelCase-значения, в частности `runState: "waitingInput"` (сериализация
  `RunState::WaitingInput` через `serde(rename_all = "camelCase")`) и поле
  `sensitive` в `pendingInput`.
- Примеры запросов/ответов согласованы с фактическими обработчиками и
  `WiKi/todo/phase-6/consulting 5/api.md`.

## Расхождения с `consulting 5/api.md`, учтённые в документе

| Пункт | В `api.md` | Фактически в коде | В `REST-API.help.md` |
|---|---|---|---|
| Состояние ожидания ввода | `waiting_input` | `waitingInput` (`serde` camelCase) | `waitingInput` |
| Поле `pendingInput.sensitive` | отсутствует | есть, `#[serde(default)]`, всегда сериализуется | добавлено |
| Защита логов сессии | только активные `log.md`/`dump.json` | весь каталог `log-folder` попадает в deny-list | описан фактический deny-list по каталогу |

Фактический код обработчиков принят как основной источник истины, поскольку
проверка корректности требует соответствия DTO и реальному роутеру.

## Проверка

1. Файл `REST-API.help.md` создан в корне репозитория.
2. Документ на английском языке, русскоязычных фрагментов нет.
3. Все перечисленные эндпоинты присутствуют в реальном роутере
   `crates/cli-harness/src/server/handlers.rs`:
   - `GET /api/v1/health`
   - `POST /api/v1/run`
   - `GET /api/v1/run/current`
   - `POST /api/v1/run/stop`
   - `POST /api/v1/run/input`
   - `GET|PUT /api/v1/config`
   - `POST /api/v1/config/default`
   - `GET /api/v1/fs/list`
   - `GET /api/v1/fs/read`
   - `PUT /api/v1/fs/write`
   - `POST /api/v1/fs/mkdir`
   - `DELETE /api/v1/fs/remove`
   - `POST /api/v1/fs/move`
4. Примеры ответов соответствуют DTO (`harness-api`), а структура ошибок —
   `ApiError`/`ConfigInvalidResponse`.
5. OpenAPI-блок структурно корректен: `openapi: 3.0.3`, `info`, `servers`,
   `security`, `paths`, `components.securitySchemes`, `components.schemas`,
   `components.responses`; ссылки `$ref` указывают на определённые компоненты.
