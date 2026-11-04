# Задача на рефакторинг: Синхронизация документации с фактическим поведением phase-7

## Цель рефакторинга:
Привести пользовательскую и API-документацию в соответствие с реализованным в phase-7 поведением: CWD-first (ключ `cwd` больше не задаётся в `harness.json5`), новые имена release-артефактов, правило `--conf` внутри CWD, `deprecated-dir-patterns` и актуальные метаданные OpenAPI. Код не меняется.

## Вносимые изменения:

### Изменение №1: `README.md`
Привести `README.md` в соответствие с `crates/cli-harness/help.txt`, `.github/workflows/ci.yml` и фактическим поведением phase-7.

- Блок CLI: заменить строку
  `harness.exe [--cwd <path>] [--api-key <key>] --dump "<path to dump file>"`
  на
  `harness.exe [--api-key <key>] --dump "<path to dump file>"`,
  так как `--cwd` с `--dump` запрещён (CWD берётся из dump-файла).
- Строки про release-артефакты (раздел Build и раздел CI): заменить
  `harness-<version>.exe` на `harness-<version>-windows.exe`.
- Правило для `--conf`: указать, что default в обоих режимах (console и server) —
  `<cwd>/harness.json5`; относительный `--conf` резолвится от process cwd,
  затем обязан находиться внутри итогового CWD.
- Раздел Configuration:
  - убрать описание ключа `cwd` как поля `harness.json5`; явно указать, что
    CWD задаётся только через `--cwd` или process cwd, а ключ `cwd` в конфиге
    отвергается;
  - для `agent.config` указать: относительные пути резолвятся от CWD и
    обязаны быть внутри CWD.
- Раздел Tools для `fs`: добавить `deprecated-dir-patterns` к списку полей
  (`available-dirs`, `available-file-extensions`, `available-files`,
  `deprecated-dirs`, `deprecated-files`, `deprecated-dir-patterns`).
- Раздел CI:
  - заменить перечисление артефактов на `harness-<version>-windows.exe`,
    `harness-<version>-linux`, `harness-<version>-macos`;
  - указать, что GitHub Release создаётся только для тегов `v*`, с проверкой
    совпадения тега и версии из `Cargo.toml`, и публикуются `.sha256` файлы.

### Изменение №2: `crates/tools/help-user.txt`
- В разделе «Variables» убрать/заменить пример с корневым ключом:
  `"cwd": "${vars:work-dir}"`. Зафиксировать, что CWD не задаётся в конфиге,
  а берётся из `--cwd` или process cwd; `vars` по-прежнему доступны.
- В разделах `skills` и `roles`: заменить формулировку
  «Relative paths are resolved against the directory of harness.json5»
  на «Relative paths are resolved against CWD and must stay inside CWD».
- В разделе «Access model»: заменить строку
  «Relative paths in these lists are resolved against the current working
  directory of the process, not settings.cwd» на корректную: относительные
  `available-dirs` / `available-files` / `deprecated-*` резолвятся от CWD и
  обязаны оставаться внутри CWD.

### Изменение №3: `api/v1/REST-API.help.md`
- Исправить опечатку в начале: «any HTTP lient that supports HTTPS» →
  «any HTTP client that supports HTTPS».
- Пример ошибки `PUT /api/v1/config`: заменить `"message": "missing 'cwd'"`
  на сообщение, соответствующее phase-7 (например, о том, что `cwd` больше
  не задаётся в конфиге и нужно использовать `--cwd`/process cwd).
- Примеры `GET /api/v1/config` и `PUT /api/v1/config`: убрать
  `"cwd": "C:/MyLife/Harness"` из тела конфига.
- Пример `GET /api/v1/health`: обновить `"version"` до актуальной версии
  workspace (`2.1.0`, берётся из `Cargo.toml`).

### Изменение №4: `api/v1/openapi.yaml`
- `info.version`: заменить `1.5.0` на `2.1.0` (workspace-версия из
  корневого `Cargo.toml`).
- `servers[].variables.port.default`: заменить `'443'` на `'4067'`
  (фактический default-порт сервера).

## Проверка корректности рефакторинга:
- Изменяются только документационные файлы; `cargo check --workspace` не
  затрагивается и должен оставаться зелёным.
- Проверить отсутствие устаревших формулировок:
  - в `README.md` нет `--cwd <path>] [--api-key <key>] --dump` и нет
    `harness-<version>.exe` (кроме, при необходимости, пояснений в тексте
    задачи);
  - в `crates/tools/help-user.txt` нет примера `"cwd": "${vars:work-dir}"`
    и нет формулировки «resolved against the current working directory of
    the process, not settings.cwd»;
  - в `api/v1/REST-API.help.md` нет `"cwd": "C:/MyLife/Harness"` в телах
    конфига и нет `"message": "missing 'cwd'"`;
  - в `api/v1/openapi.yaml` версия `2.1.0` и порт `4067`.

## Scope
Минимально необходимые изменения:

- `README.md`
- `crates/tools/help-user.txt`
- `api/v1/REST-API.help.md`
- `api/v1/openapi.yaml`

Справочные материалы для чтения:

- `crates/cli-harness/help.txt`
- `.github/workflows/ci.yml`
- `Cargo.toml`
- `crates/conf/settings-template.json5`
- `WiKi/todo/phase-7/pm-consulting report 3/report from refactoring manager.md`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 5-sync-documentation-with-phase-7 → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
