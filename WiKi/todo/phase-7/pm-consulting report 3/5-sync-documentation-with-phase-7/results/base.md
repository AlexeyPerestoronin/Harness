# Результат: Синхронизация документации с фактическим поведением phase-7

## Область изменений

Изменялись только документационные файлы:

- `README.md`
- `crates/tools/help-user.txt`
- `api/v1/REST-API.help.md`
- `api/v1/openapi.yaml`

Код не менялся.

## Было / Стало / Почему

### 1. `README.md`

- **Было:** CLI-блок содержал `harness.exe [--cwd <path>] [--api-key <key>] --dump ...`.
  **Стало:** `harness.exe [--api-key <key>] --dump ...`.
  **Почему:** в phase-7 `--cwd` запрещён вместе с `--dump`; CWD берётся из dump-файла.

- **Было:** release-артефакт Windows назывался `harness-<version>.exe`.
  **Стало:** `harness-<version>-windows.exe`.
  **Почему:** фактическое имя артефакта в `.github/workflows/ci.yml`.

- **Было:** default `--conf` описан как `<process-cwd>/harness.json5` для console и `<cwd>/harness.json5` для server.
  **Стало:** default `<cwd>/harness.json5` в обоих режимах; относительный `--conf` резолвится от process cwd и обязан быть внутри итогового CWD.
  **Почему:** поведение phase-7 CWD-first.

- **Было:** `cwd` описан как поле `harness.json5`; `agent.config` резолвился от директории конфига.
  **Стало:** CWD задаётся только `--cwd`/process cwd, ключ `cwd` в конфиге отвергается; `agent.config` резолвится от CWD и обязан быть внутри CWD.
  **Почему:** CWD-first и валидация корневых ключей конфига.

- **Было:** в списке полей `tools.fs` не было `deprecated-dir-patterns`.
  **Стало:** поле добавлено.
  **Почему:** phase-7 добавил glob-денyлист `deprecated-dir-patterns`.

- **Было:** CI описывал артефакт `harness-<version>.exe` и Release для любого тега.
  **Стало:** `harness-<version>-windows.exe`; GitHub Release создаётся только для `v*`, с проверкой совпадения тега и версии `Cargo.toml`, публикуются `.sha256`.
  **Почему:** фактическое поведение release job.

### 2. `crates/tools/help-user.txt`

- **Было:** в примере Variables был корневой ключ `"cwd": "${vars:work-dir}"`.
  **Стало:** пример без `cwd`; добавлено пояснение, что CWD берётся из `--cwd`/process cwd, корневой `cwd` — ошибка загрузки, `vars` доступны.
  **Почему:** корневой `cwd` в конфиге отвергается.

- **Было:** в `skills`/`roles` пути резолвились «against the directory of harness.json5».
  **Стало:** «against CWD and must stay inside CWD».
  **Почему:** phase-7 резолвит пути от CWD с проверкой «внутри CWD».

- **Было:** в Access model пути `available-dirs`/`available-files`/`deprecated-*` резолвились от process cwd, «not settings.cwd».
  **Стало:** относительные пути этих списков резолвятся от CWD и обязаны оставаться внутри CWD.
  **Почему:** фактическое поведение `Sandbox`/`conf::resolve_in_cwd`.

### 3. `api/v1/REST-API.help.md`

- **Было:** опечатка «HTTP lient».
  **Стало:** «HTTP client».

- **Было:** пример ошибки `PUT /api/v1/config` содержал `"message": "missing 'cwd'"`.
  **Стало:** `"message": "'cwd' is no longer set in the config; use '--cwd' or run the process from the target directory"`.
  **Почему:** фактическое сообщение валидации phase-7.

- **Было:** в телах примеров `GET /api/v1/config` и `PUT /api/v1/config` был `"cwd": "C:/MyLife/Harness"`.
  **Стало:** ключ `cwd` удалён из тел конфига.
  **Почему:** `cwd` больше не является полем `harness.json5`.

- **Было:** в `GET /api/v1/health` была `"version": "1.5.0"`.
  **Стало:** `"version": "2.1.0"`.
  **Почему:** актуальная workspace-версия из корневого `Cargo.toml`.

### 4. `api/v1/openapi.yaml`

- **Было:** `info.version: 1.5.0`.
  **Стало:** `info.version: 2.1.0`.
  **Почему:** актуальная workspace-версия.

- **Было:** `servers[].variables.port.default: '443'`.
  **Стало:** `servers[].variables.port.default: '4067'`.
  **Почему:** фактический default-порт сервера.
