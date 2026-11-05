# REST API Help

This document is the human-readable reference for the `harness.exe --serve` HTTPS REST API. All examples use `camelCase` JSON field names and can be executed with any HTTP client that supports HTTPS and bearer
tokens.

---

## 1. General information

### Base URL and transport

- Transport: **HTTPS only**. Plain HTTP, SSE and WebSocket are not used.
- Base path: all endpoints are under **`/api/v1`**.
- Base URL: `https://<host>:<port>/api/v1`, where `<host>` and `<port>` are the
  values configured for `harness.exe --serve` in `harness.json5`.
- Data format: JSON, UTF-8.
- Request `Content-Type` must be `application/json` when a request body is sent.

### Authentication

- `GET /api/v1/health` is **public**.
- Every other endpoint requires a bearer token in the `Authorization` header:

```text
Authorization: Bearer <token>
```

- The token is accepted **only** in the `Authorization` header. Missing tokens,
  invalid tokens and non-bearer schemes are rejected with `401 Unauthorized`.

### Error format

All failed endpoints return a JSON body in the following shape:

```json
{
  "error": {
    "code": "INVALID_PROMPT",
    "message": "prompt must not be empty"
  }
}
```

`code` is a stable machine-readable string, `message` is a human-readable
description.

Exception: `PUT /api/v1/config` validation failures return `400` with the body

```json
{
  "ok": false,
  "error": {
    "code": "CONFIG_INVALID",
    "message": "'cwd' is no longer set in the config; use '--cwd' or run the process from the target directory"
  }
}
```

### HTTP status codes

| Code | Meaning |
|---|---|
| `200` | Success; response body contains the result |
| `202` | Request accepted; processing started or continues |
| `204` | Success without a body (CORS preflight) |
| `400` | Invalid request or failed validation |
| `401` | Missing, invalid or incorrectly formed bearer token |
| `403` | Path protected by the FS deny-list or operation not allowed |
| `404` | Resource not found |
| `409` | State conflict (for example, a run is already active) |
| `412` | Configuration is missing or invalid |
| `503` | Server cannot perform the operation |

### CORS

- CORS is disabled by default.
- When `cors-allowed-origins` is configured, CORS headers are returned only for
  an **exact** `Origin` match.
- For allowed origins, preflight `OPTIONS` requests return `204` with:

```text
Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS
Access-Control-Allow-Headers: Authorization, Content-Type
```

- Preflight is handled before authentication, so `OPTIONS` never requires a
  bearer token.

---

## 2. Health

### `GET /api/v1/health` (public)

Returns server availability and state. Intentionally does not expose `cwd`,
`port`, `liveTimeoutMinutes` or other environment details.

**Response `200`:**

```json
{
  "status": "ok",
  "version": "2.1.0",
  "serverTime": "2026-01-01T12:00:00Z",
  "authRequired": true,
  "runState": "idle",
  "configState": "ok"
}
```

| Field | Values |
|---|---|
| `status` | Always `"ok"` |
| `version` | Server version from the crate version |
| `serverTime` | ISO-8601 UTC server time |
| `authRequired` | Always `true` (protected routes require a token) |
| `runState` | `idle`, `running`, `waitingInput`, `stopping`, `finished`, `failed`, `stopped` |
| `configState` | `ok`, `missing`, `invalid` |

---

## 3. Run lifecycle

The server supports **one active run session at a time**. Run states are
serialized in `camelCase`:

`idle` → `running` → (`waitingInput` | `finished` | `failed`), and
`running`/`waitingInput` → `stopping` → `stopped`.

The terminal state of the last session (`finished`, `failed` or `stopped`)
remains available from `GET /api/v1/run/current` until the next
`POST /api/v1/run`.

### `POST /api/v1/run`

Starts the agent loop. The web client does not select an agent or provide API
keys; the server resolves them itself or asks the client through
`pendingInput`.

**Request:**

```json
{
  "prompt": "Your prompt text"
}
```

**Response `202`:**

```json
{
  "runState": "running",
  "runUrl": "/api/v1/run/current"
}
```

`runUrl` is the polling hook for the current session. Because the server keeps
only one active session, the hook points to the current session and remains
valid until the server stops; after the session finishes it continues to
return the terminal state until the next `POST /api/v1/run`.

**Errors:**

| Code | Reason |
|---|---|
| `400 INVALID_PROMPT` | `prompt` is empty or whitespace-only |
| `401` | Token missing or invalid |
| `409 RUN_ALREADY_ACTIVE` | A run is already active |
| `412 CONFIG_MISSING` | `harness.json5` does not exist |
| `412 CONFIG_INVALID` | `harness.json5` is invalid |

### `GET /api/v1/run/current`

Returns the current/last session state. This is the main polling endpoint.

**Response `200` when no run has ever been started:**

```json
{
  "runState": "idle"
}
```

**Response `200` when a run is active:**

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

**Response `200` when the server waits for user input:**

```json
{
  "runState": "waitingInput",
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
    "prompt": "Enter the API key for DeepSeek-V3:",
    "timeoutAt": "2026-01-01T12:15:00Z",
    "sensitive": true
  }
}
```

**Response `200` when the run is terminal:**

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

Fields:

| Field | Presence |
|---|---|
| `runState` | Always present |
| `startedAt` | Present when a session exists; ISO-8601 UTC |
| `updatedAt` | Present when a session exists; ISO-8601 UTC |
| `summary` | Present when a session exists; run counters |
| `pendingInput` | Present only in `waitingInput` |
| `result` | Present only in terminal states |

`summary` and `stats` fields:

| Field | Type |
|---|---|
| `iteration` | integer |
| `tokens` | integer |
| `usdApprox` | number |
| `usdReal` | number or `null` |

`pendingInput` fields:

| Field | Type | Values |
|---|---|---|
| `requestId` | string | ULID with `req_` prefix (26 characters after the prefix) |
| `kind` | string | `text` — arbitrary answer; `confirm` — `y`/`n` confirmation |
| `prompt` | string | Question, agent choice, API-key request or confirmation text |
| `timeoutAt` | string | ISO-8601 UTC expiration of `interaction-timeout` |
| `sensitive` | boolean | Explicit flag for secret input (for example, an API key); default `false` |

### `POST /api/v1/run/stop`

Requests run cancellation. **Idempotent.**

**Request body is optional:**

```json
{
  "reason": "user changed their mind"
}
```

**Responses:**

| Situation | Response |
|---|---|
| Run is `running` or `waitingInput` | `202 {"runState":"stopping"}` |
| Already `stopping` or `stopped` | `202 {"runState":"stopping"}` |
| No active run | `200 {"runState":"idle"}` |

Behavior:

- the run `CancellationToken` is cancelled;
- an in-flight vendor HTTP request/body read is aborted;
- a pending input wait is cancelled;
- after cancellation the run state becomes `stopped` and the terminal `result`
  is returned by `GET /api/v1/run/current`.

### `POST /api/v1/run/input`

Submits an answer for the active `pendingInput`.

**Request:**

```json
{
  "requestId": "req_01J9Z8X7V6T5R4P3N2M1Q",
  "answer": "Your answer"
}
```

**Response `202`:**

```json
{
  "accepted": true
}
```

**Validation:**

- `kind = text`: `answer` must be a non-empty string.
- `kind = confirm`: `answer` must be `y`, `n`, `yes` or `no` (case-insensitive).
  It is normalized by the server to `y` or `n`.

**Errors:**

| Code | Reason |
|---|---|
| `400 INVALID_INPUT` | Empty `text` answer, or invalid `confirm` answer |
| `404 INPUT_NOT_FOUND` | No active run, no pending input, or unknown `requestId` |
| `409 INPUT_CONFLICT` | Input is no longer pending, already answered, cancelled or timed out |

---

## 4. Config API

The Config API manages the server's `harness.json5` file.

Secret redaction:

- values of the exact keys `api-key`, `apikey`, `token`, `secret`,
  `private-key` and `tls-key` are replaced by `"***"` on read;
- redaction is applied to the parsed JSON5/JSON document with exact key-name
  matching;
- if the file is invalid and cannot be parsed, raw text is **not** returned:
  `text` is `null` and `validationError` contains the parse/validation message.

### `GET /api/v1/config`

Returns the current configuration with secret redaction.

**Response `200` when the file is valid:**

```json
{
  "path": "C:/MyLife/Harness/harness.json5",
  "exists": true,
  "text": "{\n  ...\n}\n",
  "valid": true,
  "validationError": null
}
```

**Response `200` when the file is missing:**

```json
{
  "path": "C:/MyLife/Harness/harness.json5",
  "exists": false,
  "text": "{\n  ...default template...\n}\n",
  "valid": false,
  "validationError": null
}
```

| Field | Description |
|---|---|
| `path` | Absolute path of `harness.json5` |
| `exists` | Whether the file exists |
| `text` | Redacted config text, default template when missing, or `null` when invalid |
| `valid` | Whether the current config is valid |
| `validationError` | Validation message when invalid, otherwise `null` |

### `PUT /api/v1/config`

Validates and saves a new configuration. The file is saved **only** if
validation succeeds.

**Request:**

```json
{
  "text": "{\n  ...\n}\n"
}
```

**Success `200`:**

```json
{
  "ok": true,
  "path": "C:/MyLife/Harness/harness.json5",
  "valid": true
}
```

**Failure `400`:**

```json
{
  "ok": false,
  "error": {
    "code": "CONFIG_INVALID",
    "message": "'cwd' is no longer set in the config; use '--cwd' or run the process from the target directory"
  }
}
```

Secret merge rule:

- applied only when the existing file is successfully parsed as JSON5/JSON;
- if a secret field is `"***"` or `"<redacted>"`, the previous value from the
  existing file is preserved;
- a new real secret value can be written; it is never returned on read;
- if the file does not exist or cannot be parsed and the input contains
  `"***"` for a secret field, the server returns `400` because a redacted
  value cannot be used to create a new file.

### `POST /api/v1/config/default`

Creates a default `harness.json5` when the file is missing; otherwise returns
the current configuration. **Idempotent.**

**Response `200`:**

```json
{
  "path": "C:/MyLife/Harness/harness.json5",
  "exists": true,
  "text": "{\n  ...\n}\n",
  "valid": true
}
```

---

## 5. FS API

The FS API operates only inside the configured `cwd` tree. All paths are
relative.

### Path rules

- `path`, `src` and `dst` are relative to `cwd`.
- The server normalizes the path and rejects:
  - absolute paths;
  - `..` components;
  - symlinks or traversals that escape `cwd`.
- Text files are read and written as UTF-8.
- A deny-list check runs before any file access.

`GET /api/v1/fs/list` may use an empty path to list the `cwd` root; other
FS endpoints require a non-empty path.

### Deny-list

The following paths are always unavailable for the FS API (`403 FORBIDDEN_PATH`):

- `.harness/**`;
- `harness.json5` (configuration is managed only through `/api/v1/config`);
- the configured session log folder (`log-folder`) and everything inside it;
- secret-like files: `.env`, `*.pem`, `*.key`, `*.p12`, `*.pfx`, `id_rsa`,
  `id_ed25519`.

For `GET /api/v1/fs/list`, denied entries are silently omitted from the
directory listing; for direct operations the server returns `403`.

### Redaction and merge for text files

- Redaction on read and merge on write apply **only** to files that parse as
  JSON5/JSON.
- Secret keys: `api-key`, `apikey`, `token`, `secret`, `private-key`,
  `tls-key`; comparison is by exact key name, not substring.
- Other text files (Markdown, YAML, source code, etc.) are returned and saved
  as-is unless protected by the deny-list.
- If a file exists but is not parsed JSON5/JSON, merge is not applied and
  `PUT` overwrites it with the new content.

### `GET /api/v1/fs/list?path=<rel>`

Lists directory contents.

**Example request:** `GET /api/v1/fs/list?path=roles`

**Response `200`:**

```json
{
  "path": "roles",
  "entries": [
    {"name": "architect.md", "kind": "file", "size": 1024},
    {"name": "developer.md", "kind": "file", "size": 2048}
  ]
}
```

### `GET /api/v1/fs/read?path=<rel>`

Reads a text file. JSON5/JSON files are returned redacted and pretty-printed.

**Example request:** `GET /api/v1/fs/read?path=roles/architect.md`

**Response `200`:**

```json
{
  "path": "roles/architect.md",
  "text": "# Role: Architect\n..."
}
```

**Example request:** `GET /api/v1/fs/read?path=harness/agents/deepseek-v3.json5`

**Response `200`:**

```json
{
  "path": "harness/agents/deepseek-v3.json5",
  "text": "{\n  \"vendor\": \"DeepSeek\",\n  \"api-key\": \"***\"\n}\n"
}
```

### `PUT /api/v1/fs/write?path=<rel>`

Creates or overwrites a text file.

**Request:**

```json
{
  "text": "# Role: Architect\n..."
}
```

**Response `200`:**

```json
{
  "ok": true,
  "path": "roles/architect.md"
}
```

If a JSON5/JSON file contains a redacted secret marker, the previous secret
value from the existing file is preserved. If the file is new and a redacted
marker is supplied, the server returns `400 REDACTED_SECRET`.

### `POST /api/v1/fs/mkdir?path=<rel>`

Creates a directory. Idempotent when the directory already exists.

**Example request:** `POST /api/v1/fs/mkdir?path=skills`

**Response `200`:**

```json
{
  "ok": true,
  "path": "skills"
}
```

### `DELETE /api/v1/fs/remove?path=<rel>`

Removes a file or an empty directory.

**Example request:** `DELETE /api/v1/fs/remove?path=skills/old-skill.md`

**Response `200`:**

```json
{
  "ok": true,
  "path": "skills/old-skill.md"
}
```

Removing a non-empty directory returns `409 DIR_NOT_EMPTY`.

### `POST /api/v1/fs/move?src=<rel>&dst=<rel>`

Moves or renames an entry. Missing destination parent directories are created.

**Example request:** `POST /api/v1/fs/move?src=roles/old.md&dst=roles/new.md`

**Response `200`:**

```json
{
  "ok": true,
  "src": "roles/old.md",
  "dst": "roles/new.md"
}
```

### FS errors

| Code | Status | Reason |
|---|---|---|
| `INVALID_PATH` | `400` | Absolute path, `..`, escape through a symlink, or empty path where not allowed |
| `FORBIDDEN_PATH` | `403` | Path is protected by the deny-list |
| `FS_NOT_FOUND` | `404` | File or directory not found |
| `FS_IO` | `400` | Not a directory/file, non-UTF-8 content, or another FS operation error |
| `DIR_NOT_EMPTY` | `409` | Cannot remove a non-empty directory |
| `REDACTED_SECRET` | `400` | A redacted secret cannot be used for a new file |

---

## 6. Client polling scenario

A typical client session:

1. **Start a run**

   ```http
   POST /api/v1/run
   Authorization: Bearer <token>
   Content-Type: application/json

   {"prompt": "Do the work"}
   ```

   The client receives `202` with `runUrl` (for the single-session server this
   is `/api/v1/run/current`).

2. **Poll every 10 seconds**

   ```http
   GET /api/v1/run/current
   Authorization: Bearer <token>
   ```

   Repeat while `runState` is `running` or `stopping`.

3. **Answer pending input when requested**

   When the response contains `pendingInput` with `runState: "waitingInput"`,
   submit the answer:

   ```http
   POST /api/v1/run/input
   Authorization: Bearer <token>
   Content-Type: application/json

   {
     "requestId": "req_01J9Z8X7V6T5R4P3N2M1Q",
     "answer": "y"
   }
   ```

   Then continue polling.

4. **Read the terminal result**

   When `runState` is `finished`, `failed` or `stopped`, the response contains
   `result`. The terminal state is retained until the next `POST /api/v1/run`.

---

## 7. Endpoint summary

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/api/v1/health` | no | Server state |
| `POST` | `/api/v1/run` | yes | Start a run; returns `runUrl` |
| `GET` | `/api/v1/run/current` | yes | Poll current/last run state |
| `POST` | `/api/v1/run/stop` | yes | Stop the run (idempotent) |
| `POST` | `/api/v1/run/input` | yes | Answer `pendingInput` |
| `GET` | `/api/v1/config` | yes | Read config (redacted) |
| `PUT` | `/api/v1/config` | yes | Save config |
| `POST` | `/api/v1/config/default` | yes | Create default config (idempotent) |
| `GET` | `/api/v1/fs/list` | yes | List directory |
| `GET` | `/api/v1/fs/read` | yes | Read file |
| `PUT` | `/api/v1/fs/write` | yes | Write file |
| `POST` | `/api/v1/fs/mkdir` | yes | Create directory |
| `DELETE` | `/api/v1/fs/remove` | yes | Remove file/empty directory |
| `POST` | `/api/v1/fs/move` | yes | Move/rename |
