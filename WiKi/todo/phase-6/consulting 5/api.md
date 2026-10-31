# Phase 6. REST API-контракт

> Документ описывает HTTP-контракт HTTPS-сервера `harness.exe --serve`.
> Базовый путь: `/api/v1`.
> Транспорт: только HTTPS + REST (JSON). SSE/WebSocket не используются.
> Смежные документы: `base.md`, `arch.md`, `tech.md`, `ui.md`, `auth.md`.

---

## 1. Общие правила

- Протокол: HTTPS.
- Формат данных: JSON.
- Кодировка: UTF-8.
- Аутентификация: Bearer-токен для всех эндпоинтов, кроме `GET /api/v1/health`; токен передаётся **только** в заголовке `Authorization`.
- CORS по умолчанию запрещён; при настроенном allowlist заголовки CORS отправляются только для точного совпадения origin.
- Для разрешённых origin сервер поддерживает preflight: `Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS`, `Access-Control-Allow-Headers: Authorization, Content-Type`.
- Все пути файловой системы ограничены `cwd`; действует deny-list и redaction (см. §5).
- Клиент опрашивает URL-hook сессии с фиксированным интервалом 10 секунд.

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
| `403` | Путь запрещён deny-list'ом или операция недопустима |
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
  "configState": "ok"
}
```

Поля:

| Поле | Значения |
|---|---|
| `authRequired` | всегда `true` |
| `runState` | `idle`, `running`, `waiting_input`, `stopping`, `finished`, `failed`, `stopped` |
| `configState` | `ok`, `missing`, `invalid` |

Эндпоинт намеренно не возвращает `cwd`, `port`, `liveTimeoutMinutes` и другие детали окружения: они доступны только аутентифицированным клиентам через `GET /config` (при использовании `fs-panel`).

---

## 3. Запуск, статус и управление циклом

### `POST /api/v1/run`

Запускает агентский цикл. Web-клиент не выбирает агента и не передаёт API-ключ: всё это сервер решает сам или запрашивает через `pendingInput`.

**Запрос:**

```json
{
  "prompt": "Текст промпта"
}
```

**Ответ `202`:**

```json
{
  "runState": "running",
  "runUrl": "/api/v1/run/current"
}
```

`runUrl` — персональный URL-hook сессии, который клиент опрашивает каждые 10 секунд. Так как сервер поддерживает одну активную сессию, hook указывает на текущую сессию и действует до остановки сервера; после завершения сессии он продолжает возвращать её терминальное состояние до следующего `POST /run`.

**Ошибки:**

| Код | Причина |
|---|---|
| `400` | Пустой промпт |
| `401` | Токен отсутствует/неверен |
| `409` | Уже выполняется активный цикл |
| `412` | Конфигурация отсутствует или невалидна |
| `503` | Не удалось создать/запустить цикл |

### `GET /api/v1/run/current`

Возвращает состояние текущей/последней сессии. Это основной polling-эндпоинт клиента.

**Ответ `200`, когда сессия ещё не запускалась:**

```json
{
  "runState": "idle"
}
```

**Ответ `200`, когда сессия активна:**

```json
{
  "runState": "running",
  "startedAt": "2026-01-01T12:00:00Z",
  "updatedAt": "2026-01-01T12:05:00Z",
  "summary": {
    "iteration": 3,
    "tokens": 12345,
    "usdApprox": 0.12,
    "usdReal": 0.11
  }
}
```

**Ответ `200`, когда сервер ожидает ввод пользователя:**

```json
{
  "runState": "waiting_input",
  "startedAt": "2026-01-01T12:00:00Z",
  "updatedAt": "2026-01-01T12:05:00Z",
  "summary": {
    "iteration": 2,
    "tokens": 8000,
    "usdApprox": 0.08,
    "usdReal": 0.07
  },
  "pendingInput": {
    "requestId": "req_01J9Z8X7V6T5R4P3N2M1Q",
    "kind": "text",
    "prompt": "Введите API-ключ для DeepSeek-V3:",
    "timeoutAt": "2026-01-01T12:15:00Z"
  }
}
```

**Ответ `200`, когда сессия завершена:**

```json
{
  "runState": "finished",
  "startedAt": "2026-01-01T12:00:00Z",
  "updatedAt": "2026-01-01T12:20:00Z",
  "summary": {
    "iteration": 3,
    "tokens": 12345,
    "usdApprox": 0.12,
    "usdReal": 0.11
  },
  "result": {
    "ok": true,
    "runState": "finished",
    "stats": {
      "iteration": 3,
      "tokens": 12345,
      "usdApprox": 0.12,
      "usdReal": 0.11
    },
    "error": null
  }
}
```

Поля `pendingInput` и `result` присутствуют только в соответствующих состояниях. Терминальное состояние (`finished`/`failed`/`stopped`) сохраняется до следующего `POST /run`, чтобы клиент гарантированно увидел результат даже при медленном polling.

Поля `pendingInput`:

| Поле | Тип | Значения |
|---|---|---|
| `requestId` | string | ULID с префиксом `req_` (26 символов после префикса) |
| `kind` | string | `text` — произвольный ответ; `confirm` — подтверждение `y`/`n` |
| `prompt` | string | Текст запроса (вопрос, выбор агента, запрос API-ключа, подтверждение) |
| `timeoutAt` | string (ISO-8601 UTC) | Момент истечения `interaction-timeout` |

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
- если цикл ждал ввод, ожидание завершается ошибкой отмены;
- после остановки `runState` становится `stopped`, в `result` возвращается соответствующий итог.

### `POST /api/v1/run/input`

Передаёт ответ на активный `pendingInput` (вопрос агента, выбор агента, ввод API-ключа, подтверждение).

**Запрос:**

```json
{
  "requestId": "req_01J9Z8X7V6T5R4P3N2M1Q",
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
| `400` | `kind = text`, но `answer` пустой; `kind = confirm`, но `answer` не `y`/`n`/`yes`/`no` |
| `404` | Нет ожидающего ввода с таким `requestId` |
| `409` | Ввод уже отвечен, отменён или истёк таймаут |

Ответ `answer` для `kind = confirm` нормализуется сервером: `y`/`yes` → `y`, `n`/`no` → `n` (регистр не учитывается). Для `kind = text` ответ должен быть непустой строкой.

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

- значения ключей `api-key`, `apikey`, `token`, `secret`, `private-key`, `tls-key` заменяются на `"***"`;
- redaction выполняется по разобранному JSON5/JSON с точным совпадением имён ключей;
- если конфигурация невалидна и не может быть разобрана, сервер **не возвращает** сырой текст: `text: null`, `validationError` содержит сообщение разбора.

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

- применяется только если существующий файл успешно разобран как JSON5/JSON;
- если значение секретного поля равно `"***"` или `"<redacted>"`, сохраняется прежнее значение из существующего файла;
- новое фактическое значение секретного поля записать можно; обратно оно никогда не возвращается;
- если файл не существует или не разобран, а во входном тексте встречается `"***"` для секретного поля, сервер возвращает `400`: редкатированное значение нельзя использовать для создания нового файла.

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

## 5. Файловая система внутри `cwd`

> В первой версии standalone-клиент не использует FS API. Эти эндпоинты предназначены для встраиваемых приложений с фичей `fs-panel`.

API работает только с деревом `cwd`. Пути всегда относительные.

### Общие правила

- `path`, `src`, `dst` — относительные пути от `cwd`.
- Сервер нормализует путь и отклоняет:
  - абсолютные пути;
  - `..`;
  - symlink/переходы, выходящие за пределы `cwd`.
- Текстовые файлы читаются/пишутся как UTF-8.
- Перед доступом к файлу выполняется проверка deny-list.

### Deny-list

Следующие пути всегда недоступны для FS API (код `403 FORBIDDEN_PATH`):

- `.harness/**`;
- `harness.json5` (конфигурация редактируется только через `/api/v1/config`);
- активные `log.md` и `dump.json` текущей сессии, пока `runState` — `running`/`waiting_input`/`stopping`;
- секретоподобные файлы: `.env`, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `id_rsa`, `id_ed25519`.

### Redaction и merge-правило для текстовых файлов

1. Redaction на чтении и merge-правило на записи применяются **только** к файлам, которые сервер успешно разобрал как JSON5/JSON.
2. Секретные ключи: `api-key`, `apikey`, `token`, `secret`, `private-key`, `tls-key`; сравнение по точному имени, без substring-поиска.
3. Остальные текстовые файлы (Markdown, YAML, код и т.п.) читаются и сохраняются как есть, если не попадают под deny-list.
4. Если файл существует, но не является разобранным JSON5/JSON, merge-правило не применяется; `PUT` перезаписывает его новым содержимым.

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

#### Ошибка deny-list

```json
{
  "error": {
    "code": "FORBIDDEN_PATH",
    "message": "path is protected: .harness/certs/key.pem"
  }
}
```

---

## 6. Ошибки валидации конфигурации

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

## 7. Сводная таблица эндпоинтов

| Метод | Путь | Auth | Описание |
|---|---|---|---|
| `GET` | `/api/v1/health` | нет | Состояние сервера |
| `POST` | `/api/v1/run` | да | Запустить цикл; возвращает `runUrl` |
| `GET` | `/api/v1/run/current` | да | Polling-статус текущей/последней сессии |
| `POST` | `/api/v1/run/stop` | да | Остановить цикл |
| `POST` | `/api/v1/run/input` | да | Ответить на `pendingInput` |
| `GET` | `/api/v1/config` | да | Текущая конфигурация (с редкацией) |
| `PUT` | `/api/v1/config` | да | Сохранить конфигурацию |
| `POST` | `/api/v1/config/default` | да | Создать конфигурацию по умолчанию |
| `GET` | `/api/v1/fs/list` | да | Содержимое каталога |
| `GET` | `/api/v1/fs/read` | да | Прочитать файл |
| `PUT` | `/api/v1/fs/write` | да | Записать файл |
| `POST` | `/api/v1/fs/mkdir` | да | Создать каталог |
| `DELETE` | `/api/v1/fs/remove` | да | Удалить файл/каталог |
| `POST` | `/api/v1/fs/move` | да | Переместить/переименовать |
