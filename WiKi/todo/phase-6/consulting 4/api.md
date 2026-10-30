# Phase 6. REST API и SSE-контракт

> Документ описывает HTTP-контракт HTTPS-сервера `harness.exe --serve`.
> Базовый путь: `/api/v1`.
> Смежные документы: `base.md`, `arch.md`, `tech.md`, `ui.md`, `auth.md`.

---

## 1. Общие правила

- Протокол: HTTPS.
- Формат данных: JSON.
- Кодировка: UTF-8.
- Аутентификация: Bearer-токен для всех эндпоинтов, кроме `GET /api/v1/health`.
- CORS по умолчанию запрещён; при настроенном allowlist заголовки CORS отправляются только для точного совпадения origin.
- Для разрешённых origin сервер поддерживает preflight: `Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS`, `Access-Control-Allow-Headers: Authorization, Content-Type`.
- Все пути файловой системы ограничены `cwd`.
- Секреты (`api-key`, `apikey`, `token`, `secret`, `private-key`) никогда не возвращаются клиенту: в ответах они заменяются на `"***"`.

### Заголовки запроса

```text
Authorization: Bearer <token>
Content-Type: application/json
```

### Формат ошибки

```json
{
  "error": {
    "code": "INVALID_PROMPT",
    "message": "prompt must not be empty"
  }
}
```

### Коды ответов

| Код | Смысл |
|---|---|
| `200` | Успех, тело — результат |
| `202` | Запрос принят, выполнение началось/продолжается |
| `204` | Успех без тела |
| `400` | Некорректный запрос/валидация |
| `401` | Токен отсутствует, некорректен или имеет неверную схему |
| `404` | Ресурс не найден |
| `409` | Конфликт состояния (например, уже есть активный цикл) |
| `412` | Конфигурация отсутствует или невалидна |
| `503` | Сервер не может выполнить операцию |

---

## 2. Health

### `GET /api/v1/health`

Открытый эндпоинт: проверка доступности и состояния сервера.

**Ответ `200`:**

```json
{
  "status": "ok",
  "version": "1.5.0",
  "serverTime": "2026-01-01T12:00:00Z",
  "authRequired": true,
  "runState": "idle",
  "configState": "ok",
  "cwd": "C:/MyLife/Harness",
  "port": 4067,
  "liveTimeoutMinutes": 5
}
```

Поля:

| Поле | Значения |
|---|---|
| `authRequired` | всегда `true` |
| `runState` | `idle`, `running`, `waiting_input`, `stopping`, `finished`, `failed`, `stopped` |
| `configState` | `ok`, `missing`, `invalid` |

---

## 3. Агенты

### `GET /api/v1/agents`

Список доступных агентов без секретов. Требует Bearer-токен.

**Ответ `200`:**

```json
{
  "source": {
    "kind": "directory",
    "path": "C:/MyLife/Harness/harness/agents"
  },
  "agents": [
    {
      "index": 0,
      "vendor": "DeepSeek",
      "name": "DeepSeek-V3",
      "path": "C:/MyLife/Harness/harness/agents/deepseek-v3.json5",
      "tokenLimit": -1,
      "rpm": 60,
      "tpm": 100000,
      "rpd": 1000,
      "inputPrice": 0.27,
      "outputPrice": 1.10,
      "cachedPrice": 0.07,
      "apiKeyRequired": true
    }
  ]
}
```

Правила:

- `apiKeyRequired: true`, если у агента нет доступного API-ключа: ни в конфигурации, ни среди RSA-переданных ключей для данного вендора.
- После успешного `POST /api/v1/crypto/api-key` для вендора `apiKeyRequired` становится `false` до остановки сервера.
- Если `agent.config` — файл, возвращается массив из одного элемента с `index: 0`.
- Если конфигурация отсутствует/невалидна или кандидатов нет — `409`/`412`.

---

## 4. Конфигурация

> В первой версии standalone-клиент не использует эти эндпоинты. Они предназначены для встраиваемых приложений с фичей `fs-panel` и для локальной отладки.

### `GET /api/v1/config`

Возвращает текущий `harness.json5` с редкацией секретов.

**Ответ `200`:**

```json
{
  "path": "C:/MyLife/Harness/harness.json5",
  "exists": true,
  "text": "{\n  \"cwd\": \"C:/MyLife/Harness\",\n  ...\n}\n",
  "valid": true,
  "validationError": null
}
```

Правила redaction:

- значения ключей `api-key`, `apikey`, `token`, `secret`, `private-key` заменяются на `"***"`;
- если конфигурация невалидна и не может быть разобрана, сервер выполняет текстовую редкацию по тем же ключам.

Если файл отсутствует: `exists: false`, `text` содержит шаблон по умолчанию с редкацией, `valid: false`.

### `PUT /api/v1/config`

Сохраняет изменённую конфигурацию.

**Запрос:**

```json
{
  "text": "{\n  \"cwd\": \"C:/MyLife/Harness\",\n  ...\n}\n"
}
```

**Успех `200`:**

```json
{
  "ok": true,
  "path": "C:/MyLife/Harness/harness.json5",
  "valid": true
}
```

**Ошибка `400`** (валидация не пройдена, файл не сохраняется):

```json
{
  "ok": false,
  "error": {
    "code": "CONFIG_INVALID",
    "message": "missing 'cwd'"
  }
}
```

Merge-правило для секретов:

- если значение секретного поля равно `"***"` или `"<redacted>"`, сохраняется прежнее значение из существующего файла;
- новое фактическое значение секретного поля записать можно; обратно оно никогда не возвращается. Интерактивный ввод API-ключа выполняется через RSA-эндпоинт.
- если файл не существует, а во входном тексте встречается `"***"` для секретного поля, сервер возвращает `400`: редкатированное значение нельзя использовать для создания нового файла.

### `POST /api/v1/config/default`

Создаёт `harness.json5` по умолчанию, если файл отсутствует.

**Ответ `200`:**

```json
{
  "path": "C:/MyLife/Harness/harness.json5",
  "exists": true,
  "text": "{\n  ...\n}\n",
  "valid": true
}
```

Если файл уже существует — возвращает текущий (идемпотентно).

---

## 5. RSA / API-ключ

### `GET /api/v1/crypto/public-key`

Возвращает публичный ключ сервера.

**Ответ `200`:**

```json
{
  "algorithm": "RSA-OAEP-256",
  "maxApiKeyBytes": 128,
  "publicKey": "-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----"
}
```

Пара ключей генерируется при старте сервера. Приватный ключ хранится только в памяти.

### `POST /api/v1/crypto/api-key`

Передаёт зашифрованный API-ключ.

**Запрос:**

```json
{
  "agentIndex": 0,
  "encryptedApiKey": "base64(...)"
}
```

**Ответ `202`:**

```json
{
  "accepted": true,
  "vendor": "DeepSeek"
}
```

Правила:

- Сервер расшифровывает ключ приватным ключом RSA-OAEP-256.
- Открытый ключ должен быть непустым, корректным UTF-8 и длиной **не более 128 байт**; иначе — `400`.
- Сервер сохраняет **зашифрованный** ключ для конкретного вендора до остановки сервера.
- При повторной передаче для того же вендора старый шифротекст заменяется.
- При рестарте сервера все сохранённые ключи исчезают (приватный ключ был только в памяти).

---

## 6. Запуск и управление циклом

### `POST /api/v1/run`

Запускает агентский цикл.

**Запрос:**

```json
{
  "prompt": "Текст промпта",
  "agentIndex": 0
}
```

Если `agent.config` — файл (один агент), `agentIndex` можно опустить.

**Ответ `202`:**

```json
{
  "runState": "running",
  "eventsUrl": "/api/v1/run/events"
}
```

Цикл стартует сразу после `POST /run`. Клиент подключается к SSE параллельно и добирает пропущенные события из кольцевого буфера.

**Ошибки:**

| Код | Причина |
|---|---|
| `400` | Пустой промпт, неверный `agentIndex` |
| `401` | Токен отсутствует/неверен |
| `409` | Уже выполняется активный цикл |
| `412` | Конфигурация отсутствует или невалидна |
| `503` | Не удалось создать/запустить цикл |

### `GET /api/v1/run`

Возвращает состояние текущего/последнего цикла.

**Ответ `200`, когда цикл не запущен:**

```json
{
  "runState": "idle"
}
```

**Ответ `200`, когда цикл активен или завершён:**

```json
{
  "runState": "waiting_input",
  "agent": {
    "vendor": "DeepSeek",
    "name": "DeepSeek-V3"
  },
  "startedAt": "2026-01-01T12:00:00Z",
  "updatedAt": "2026-01-01T12:05:00Z",
  "summary": {
    "iteration": 3,
    "tokens": 12345,
    "usdApprox": 0.12,
    "usdReal": 0.11
  },
  "pendingInput": {
    "requestId": "req_01J...",
    "prompt": "Какое имя проекта использовать?\n"
  },
  "logFile": "C:/MyLife/Harness/logs/.../log.md",
  "dumpFile": "C:/MyLife/Harness/logs/.../dump.json"
}
```

`pendingInput` присутствует, только когда цикл ждёт ответ пользователя.

### `POST /api/v1/run/stop`

Запрашивает остановку цикла. Идемпотентен.

**Запрос (опционально):**

```json
{
  "reason": "пользователь передумал"
}
```

**Ответы:**

| Ситуация | Ответ |
|---|---|
| Цикл активен | `202 {"runState":"stopping"}` |
| Уже остановлен/останавливается | `202 {"runState":"stopping"}` |
| Нет активного цикла | `200 {"runState":"idle"}` |

Поведение:

- отменяется `CancellationToken` цикла;
- активный async HTTP-запрос/чтение тела к вендору абортируется;
- если цикл ждал `input`, ожидание завершается ошибкой отмены;
- после остановки `runState` становится `stopped`, затем сервер переходит в `idle`.

### `POST /api/v1/run/input`

Передаёт ответ на ожидающий вопрос агента.

**Запрос:**

```json
{
  "requestId": "req_01J...",
  "answer": "Текст ответа"
}
```

**Ответ `202`:**

```json
{
  "accepted": true
}
```

**Ошибки:**

| Код | Причина |
|---|---|
| `404` | Нет ожидающего ввода с таким `requestId` |
| `409` | Ввод уже отвечен, отменён или истёк таймаут |

### `GET /api/v1/run/log`

Возвращает лог текущего/последнего цикла.

Параметры query:

- `offset` — смещение в байтах (опционально);
- `tail` — последние N строк (опционально).

**Ответ `200`:** `text/plain; charset=utf-8`.

### `GET /api/v1/run/dump`

Возвращает текущий `dump.json`, если он существует.

**Ответ `200`:** `application/json`.
**Ответ `404`:** dump ещё не создан.

---

## 7. Поток событий SSE

### `GET /api/v1/run/events`

Возвращает поток событий текущего цикла: `Content-Type: text/event-stream; charset=utf-8`.

Требует Bearer-токен. Для браузерного `EventSource` токен передаётся так, как описано в `ui.md` §7.1.

### Типы событий

| Событие | Назначение | Пример `data` |
|---|---|---|
| `status` | Изменение состояния цикла/сервера | `{"runState":"running","iteration":2,"lastActivityAt":"..."}` |
| `output` | Строка вывода | `{"line":"## Iteration №2","mode":"new_line"}` |
| `input_request` | Сервер ожидает ввод пользователя | `{"requestId":"req_01J...","prompt":"...","writeLog":true}` |
| `result` | Цикл завершён/остановлен/ошибка | `{"runState":"finished","ok":true,"stats":{"iteration":3,"tokens":12345,"usdApprox":0.12,"usdReal":0.11}}` |
| `server_stopping` | Сервер завершает работу по `live-timeout` | `{"at":"..."}` |
| `keepalive` | Поддержание соединения | `{"at":"..."}` |

`mode` в `output`:

- `new_line` — новая строка лога;
- `continue_line` — продолжение текущей строки;
- `rewrite_line` — заменить последнюю строку-индикатор (таймеры, прогресс).

### Формат

```text
id: 42
event: output
data: {"line":"## Iteration №2","mode":"new_line"}

```

### Переподключение

- Каждое событие имеет монотонный `id`.
- Клиент передаёт `Last-Event-ID` (браузерный `EventSource` делает это автоматически).
- Сервер хранит кольцевой буфер последних `server.sse-buffer-size` событий (по умолчанию 512) и при подключении сначала отправляет события с `id > Last-Event-ID`, затем live-поток.
- Если клиент отстал от live-канала (`broadcast::RecvError::Lagged`), сервер отправляет `status` с `{"reconnect":true}` и закрывает поток; клиент переподключается и добирает события из кольцевого буфера.
- Если события выпали из кольцевого буфера, клиент восстанавливает состояние через `GET /run` и `GET /run/log`.

### Поведение при отсутствии активного цикла

Если активного цикла нет, `GET /api/v1/run/events` немедленно отправляет одно событие `status` с `runState: "finished"` и закрывает поток. Клиент не держит SSE-соединение в idle; новое соединение открывается на каждый запуск цикла.

---

## 8. Файловая система внутри `cwd`

> В первой версии standalone-клиент не использует FS API. Эти эндпоинты предназначены для встраиваемых приложений с фичей `fs-panel`.

API работает со всем деревом `cwd`. Пути всегда относительные.

### Общие правила

- `path`, `src`, `dst` — относительные пути от `cwd`.
- Сервер нормализует путь и отклоняет:
  - абсолютные пути;
  - `..`;
  - symlink/переходы, выходящие за пределы `cwd`.
- Текстовые файлы читаются/пишутся как UTF-8.
- Секретные поля в текстовых файлах подлежат redaction на чтении и merge-правилу на записи.

### Эндпоинты

| Метод | Путь | Назначение |
|---|---|---|
| `GET` | `/api/v1/fs/list?path=<rel>` | Содержимое каталога |
| `GET` | `/api/v1/fs/read?path=<rel>` | Чтение текстового файла |
| `PUT` | `/api/v1/fs/write?path=<rel>` | Создание/перезапись текстового файла |
| `POST` | `/api/v1/fs/mkdir?path=<rel>` | Создание каталога |
| `DELETE` | `/api/v1/fs/remove?path=<rel>` | Удаление файла/пустого каталога |
| `POST` | `/api/v1/fs/move?src=<rel>&dst=<rel>` | Перемещение/переименование |

### Примеры

#### `GET /api/v1/fs/list?path=roles`

```json
{
  "path": "roles",
  "entries": [
    {"name": "architect.md", "kind": "file", "size": 1024},
    {"name": "developer.md", "kind": "file", "size": 2048}
  ]
}
```

#### `GET /api/v1/fs/read?path=roles/architect.md`

```json
{
  "path": "roles/architect.md",
  "text": "# Роль: Architect\n..."
}
```

#### `GET /api/v1/fs/read?path=harness/agents/deepseek-v3.json5`

```json
{
  "path": "harness/agents/deepseek-v3.json5",
  "text": "{\n  \"vendor\": \"DeepSeek\",\n  \"api-key\": \"***\"\n}\n"
}
```

#### `PUT /api/v1/fs/write?path=roles/architect.md`

**Запрос:**

```json
{
  "text": "# Роль: Architect\n..."
}
```

**Ответ `200`:**

```json
{
  "ok": true,
  "path": "roles/architect.md"
}
```

#### `PUT /api/v1/fs/write?path=harness/agents/deepseek-v3.json5`

Если в запросе `"api-key": "***"`, сервер сохраняет прежнее значение ключа из существующего файла.

#### `POST /api/v1/fs/mkdir?path=skills`

```json
{
  "ok": true,
  "path": "skills"
}
```

#### `DELETE /api/v1/fs/remove?path=skills/old-skill.md`

```json
{
  "ok": true,
  "path": "skills/old-skill.md"
}
```

#### `POST /api/v1/fs/move?src=roles/old.md&dst=roles/new.md`

```json
{
  "ok": true,
  "src": "roles/old.md",
  "dst": "roles/new.md"
}
```

---

## 9. Ошибки валидации конфигурации

`PUT /api/v1/config` возвращает `400` с машиночитаемой ошибкой:

```json
{
  "ok": false,
  "error": {
    "code": "CONFIG_INVALID",
    "message": "unknown key 'toolsx' in 'settings'"
  }
}
```

UI показывает `message` пользователю и не отправляет конфигурацию повторно без исправлений.

---

## 10. Сводная таблица эндпоинтов

| Метод | Путь | Auth | Описание |
|---|---|---|---|
| `GET` | `/api/v1/health` | нет | Состояние сервера |
| `GET` | `/api/v1/agents` | да | Список агентов |
| `GET` | `/api/v1/config` | да | Текущая конфигурация (с редкацией) |
| `PUT` | `/api/v1/config` | да | Сохранить конфигурацию |
| `POST` | `/api/v1/config/default` | да | Создать конфигурацию по умолчанию |
| `GET` | `/api/v1/crypto/public-key` | да | RSA public key |
| `POST` | `/api/v1/crypto/api-key` | да | Передать зашифрованный API-ключ |
| `POST` | `/api/v1/run` | да | Запустить цикл |
| `GET` | `/api/v1/run` | да | Состояние цикла |
| `POST` | `/api/v1/run/stop` | да | Остановить цикл |
| `POST` | `/api/v1/run/input` | да | Ответить на вопрос агента |
| `GET` | `/api/v1/run/events` | да | SSE-поток событий |
| `GET` | `/api/v1/run/log` | да | Лог цикла |
| `GET` | `/api/v1/run/dump` | да | Dump текущего цикла |
| `GET` | `/api/v1/fs/list` | да | Содержимое каталога |
| `GET` | `/api/v1/fs/read` | да | Прочитать файл |
| `PUT` | `/api/v1/fs/write` | да | Записать файл |
| `POST` | `/api/v1/fs/mkdir` | да | Создать каталог |
| `DELETE` | `/api/v1/fs/remove` | да | Удалить файл/каталог |
| `POST` | `/api/v1/fs/move` | да | Переместить/переименовать |
