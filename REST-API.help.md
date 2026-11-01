# REST API Help

This document is the human-readable and machine-readable reference for the
`harness.exe --serve` HTTPS REST API. All examples use `camelCase` JSON field
names and can be executed with any HTTP client that supports HTTPS and bearer
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
    "message": "missing 'cwd'"
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
  "version": "1.5.0",
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
  "text": "{\n  \"cwd\": \"C:/MyLife/Harness\",\n  ...\n}\n",
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
  "text": "{\n  \"cwd\": \"C:/MyLife/Harness\",\n  ...\n}\n"
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
    "message": "missing 'cwd'"
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

---

## 8. OpenAPI 3.0 notation

The block below is an OpenAPI 3.0 (YAML) description of the same endpoints. It
can be used as a machine-readable reference or imported into tools such as
Postman-compatible generators.

```yaml
openapi: 3.0.3
info:
  title: Harness Server REST API
  version: 1.5.0
  description: >-
    HTTPS REST API exposed by `harness.exe --serve`. All routes except
    `/api/v1/health` require a bearer token.
servers:
  - url: https://{host}:{port}
    description: Base URL of `harness.exe --serve`. Replace with configured values.
    variables:
      host:
        default: 127.0.0.1
      port:
        default: '443'
security:
  - bearerAuth: []
paths:
  /api/v1/health:
    get:
      summary: Public server health and state
      security: []
      responses:
        '200':
          description: Server is reachable
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/HealthResponse'
  /api/v1/run:
    post:
      summary: Start a run
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/StartRunRequest'
      responses:
        '202':
          description: Run started
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/StartRunResponse'
        '400':
          $ref: '#/components/responses/BadRequest'
        '401':
          $ref: '#/components/responses/Unauthorized'
        '409':
          $ref: '#/components/responses/Conflict'
        '412':
          $ref: '#/components/responses/PreconditionFailed'
  /api/v1/run/current:
    get:
      summary: Poll current/last run state
      responses:
        '200':
          description: Current run state
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/RunStatusResponse'
        '401':
          $ref: '#/components/responses/Unauthorized'
  /api/v1/run/stop:
    post:
      summary: Stop the current run
      requestBody:
        required: false
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/StopRunRequest'
      responses:
        '200':
          description: No active run
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/RunStateResponse'
        '202':
          description: Stop accepted
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/RunStateResponse'
        '401':
          $ref: '#/components/responses/Unauthorized'
  /api/v1/run/input:
    post:
      summary: Answer a pending input
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/SubmitInputRequest'
      responses:
        '202':
          description: Answer accepted
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/SubmitInputResponse'
        '400':
          $ref: '#/components/responses/BadRequest'
        '401':
          $ref: '#/components/responses/Unauthorized'
        '404':
          $ref: '#/components/responses/NotFound'
        '409':
          $ref: '#/components/responses/Conflict'
  /api/v1/config:
    get:
      summary: Read redacted server configuration
      responses:
        '200':
          description: Configuration state
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ConfigResponse'
        '401':
          $ref: '#/components/responses/Unauthorized'
    put:
      summary: Validate and save server configuration
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/PutConfigRequest'
      responses:
        '200':
          description: Configuration saved
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/PutConfigResponse'
        '400':
          description: Configuration validation failed
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ConfigInvalidResponse'
        '401':
          $ref: '#/components/responses/Unauthorized'
  /api/v1/config/default:
    post:
      summary: Create default configuration if missing
      responses:
        '200':
          description: Default or current configuration
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ConfigDefaultResponse'
        '401':
          $ref: '#/components/responses/Unauthorized'
  /api/v1/fs/list:
    get:
      summary: List a directory inside cwd
      parameters:
        - name: path
          in: query
          required: false
          description: Relative path; empty means cwd root
          schema:
            type: string
      responses:
        '200':
          description: Directory entries
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/FsListResponse'
        '400':
          $ref: '#/components/responses/BadRequest'
        '401':
          $ref: '#/components/responses/Unauthorized'
        '403':
          $ref: '#/components/responses/Forbidden'
        '404':
          $ref: '#/components/responses/NotFound'
        '412':
          $ref: '#/components/responses/PreconditionFailed'
  /api/v1/fs/read:
    get:
      summary: Read a text file inside cwd
      parameters:
        - name: path
          in: query
          required: true
          schema:
            type: string
      responses:
        '200':
          description: File content
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/FsReadResponse'
        '400':
          $ref: '#/components/responses/BadRequest'
        '401':
          $ref: '#/components/responses/Unauthorized'
        '403':
          $ref: '#/components/responses/Forbidden'
        '404':
          $ref: '#/components/responses/NotFound'
        '412':
          $ref: '#/components/responses/PreconditionFailed'
  /api/v1/fs/write:
    put:
      summary: Create or overwrite a text file inside cwd
      parameters:
        - name: path
          in: query
          required: true
          schema:
            type: string
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/FsWriteRequest'
      responses:
        '200':
          description: File written
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/FsWriteResponse'
        '400':
          $ref: '#/components/responses/BadRequest'
        '401':
          $ref: '#/components/responses/Unauthorized'
        '403':
          $ref: '#/components/responses/Forbidden'
        '412':
          $ref: '#/components/responses/PreconditionFailed'
  /api/v1/fs/mkdir:
    post:
      summary: Create a directory inside cwd
      parameters:
        - name: path
          in: query
          required: true
          schema:
            type: string
      responses:
        '200':
          description: Directory created or already exists
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/FsMkdirResponse'
        '400':
          $ref: '#/components/responses/BadRequest'
        '401':
          $ref: '#/components/responses/Unauthorized'
        '403':
          $ref: '#/components/responses/Forbidden'
        '412':
          $ref: '#/components/responses/PreconditionFailed'
  /api/v1/fs/remove:
    delete:
      summary: Remove a file or empty directory inside cwd
      parameters:
        - name: path
          in: query
          required: true
          schema:
            type: string
      responses:
        '200':
          description: Entry removed
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/FsRemoveResponse'
        '400':
          $ref: '#/components/responses/BadRequest'
        '401':
          $ref: '#/components/responses/Unauthorized'
        '403':
          $ref: '#/components/responses/Forbidden'
        '404':
          $ref: '#/components/responses/NotFound'
        '409':
          $ref: '#/components/responses/Conflict'
        '412':
          $ref: '#/components/responses/PreconditionFailed'
  /api/v1/fs/move:
    post:
      summary: Move or rename an entry inside cwd
      parameters:
        - name: src
          in: query
          required: true
          schema:
            type: string
        - name: dst
          in: query
          required: true
          schema:
            type: string
      responses:
        '200':
          description: Entry moved
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/FsMoveResponse'
        '400':
          $ref: '#/components/responses/BadRequest'
        '401':
          $ref: '#/components/responses/Unauthorized'
        '403':
          $ref: '#/components/responses/Forbidden'
        '404':
          $ref: '#/components/responses/NotFound'
        '412':
          $ref: '#/components/responses/PreconditionFailed'
components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
      bearerFormat: token
  responses:
    BadRequest:
      description: Invalid request or validation error
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    Unauthorized:
      description: Missing, invalid or incorrectly formed bearer token
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    Forbidden:
      description: Path protected by the FS deny-list
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    NotFound:
      description: Resource not found
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    Conflict:
      description: State conflict
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    PreconditionFailed:
      description: Configuration missing or invalid
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
  schemas:
    ErrorBody:
      type: object
      required: [code, message]
      properties:
        code:
          type: string
        message:
          type: string
    ErrorResponse:
      type: object
      required: [error]
      properties:
        error:
          $ref: '#/components/schemas/ErrorBody'
    ConfigState:
      type: string
      enum: [ok, missing, invalid]
    RunState:
      type: string
      enum: [idle, running, waitingInput, stopping, finished, failed, stopped]
    HealthResponse:
      type: object
      required: [status, version, serverTime, authRequired, runState, configState]
      properties:
        status:
          type: string
        version:
          type: string
        serverTime:
          type: string
          format: date-time
        authRequired:
          type: boolean
        runState:
          $ref: '#/components/schemas/RunState'
        configState:
          $ref: '#/components/schemas/ConfigState'
    StartRunRequest:
      type: object
      required: [prompt]
      properties:
        prompt:
          type: string
    StartRunResponse:
      type: object
      required: [runState, runUrl]
      properties:
        runState:
          $ref: '#/components/schemas/RunState'
        runUrl:
          type: string
    RunSummary:
      type: object
      required: [iteration, tokens, usdApprox, usdReal]
      properties:
        iteration:
          type: integer
        tokens:
          type: integer
        usdApprox:
          type: number
        usdReal:
          type: number
          nullable: true
    PendingInputKind:
      type: string
      enum: [text, confirm]
    PendingInput:
      type: object
      required: [requestId, kind, prompt, timeoutAt, sensitive]
      properties:
        requestId:
          type: string
        kind:
          $ref: '#/components/schemas/PendingInputKind'
        prompt:
          type: string
        timeoutAt:
          type: string
          format: date-time
        sensitive:
          type: boolean
          default: false
    RunStats:
      type: object
      required: [iteration, tokens, usdApprox, usdReal]
      properties:
        iteration:
          type: integer
        tokens:
          type: integer
        usdApprox:
          type: number
        usdReal:
          type: number
          nullable: true
    RunResult:
      type: object
      required: [ok, runState, stats, error]
      properties:
        ok:
          type: boolean
        runState:
          $ref: '#/components/schemas/RunState'
        stats:
          $ref: '#/components/schemas/RunStats'
        error:
          type: string
          nullable: true
    RunStatusResponse:
      type: object
      required: [runState]
      properties:
        runState:
          $ref: '#/components/schemas/RunState'
        startedAt:
          type: string
          format: date-time
        updatedAt:
          type: string
          format: date-time
        summary:
          $ref: '#/components/schemas/RunSummary'
        pendingInput:
          $ref: '#/components/schemas/PendingInput'
        result:
          $ref: '#/components/schemas/RunResult'
    StopRunRequest:
      type: object
      properties:
        reason:
          type: string
    SubmitInputRequest:
      type: object
      required: [requestId, answer]
      properties:
        requestId:
          type: string
        answer:
          type: string
    SubmitInputResponse:
      type: object
      required: [accepted]
      properties:
        accepted:
          type: boolean
    RunStateResponse:
      type: object
      required: [runState]
      properties:
        runState:
          $ref: '#/components/schemas/RunState'
    ConfigResponse:
      type: object
      required: [path, exists, text, valid, validationError]
      properties:
        path:
          type: string
        exists:
          type: boolean
        text:
          type: string
          nullable: true
        valid:
          type: boolean
        validationError:
          type: string
          nullable: true
    PutConfigRequest:
      type: object
      required: [text]
      properties:
        text:
          type: string
    PutConfigResponse:
      type: object
      required: [ok]
      properties:
        ok:
          type: boolean
        path:
          type: string
        valid:
          type: boolean
        error:
          $ref: '#/components/schemas/ErrorBody'
    ConfigInvalidResponse:
      type: object
      required: [ok, error]
      properties:
        ok:
          type: boolean
        error:
          $ref: '#/components/schemas/ErrorBody'
    ConfigDefaultResponse:
      type: object
      required: [path, exists, text, valid]
      properties:
        path:
          type: string
        exists:
          type: boolean
        text:
          type: string
        valid:
          type: boolean
    FsEntry:
      type: object
      required: [name, kind, size]
      properties:
        name:
          type: string
        kind:
          type: string
          enum: [file, dir]
        size:
          type: integer
    FsListResponse:
      type: object
      required: [path, entries]
      properties:
        path:
          type: string
        entries:
          type: array
          items:
            $ref: '#/components/schemas/FsEntry'
    FsReadResponse:
      type: object
      required: [path, text]
      properties:
        path:
          type: string
        text:
          type: string
    FsWriteRequest:
      type: object
      required: [text]
      properties:
        text:
          type: string
    FsWriteResponse:
      type: object
      required: [ok, path]
      properties:
        ok:
          type: boolean
        path:
          type: string
    FsMkdirResponse:
      type: object
      required: [ok, path]
      properties:
        ok:
          type: boolean
        path:
          type: string
    FsRemoveResponse:
      type: object
      required: [ok, path]
      properties:
        ok:
          type: boolean
        path:
          type: string
    FsMoveResponse:
      type: object
      required: [ok, src, dst]
      properties:
        ok:
          type: boolean
        src:
          type: string
        dst:
          type: string
```
