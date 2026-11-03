# Consulting 2 — Phase 7. Базовое заключение

## Цель фазы
Доработать серверную (native) часть `cli-harness`:

1. **CWD first** — превратить `harness.exe` в системную утилиту, у которой
   рабочая директория задаётся не в конфиге, а внешним контекстом запуска.
2. **Output buffer for shell commands** — дать агенту контроль над объёмом
   возвращаемого вывода `run_shell` / `request_command_shell_execution`.
3. **Deprecated-dir-patterns** — добавить гибкий запрет директорий/файлов
   в `tools.fs` по glob-паттернам вида `**/log`, `**/done`, `**/note-*`.
4. **GitHub release action** — публикация получаемых на GitHub Actions
   дистрибутивов через `Create GitHub Release`.

## Что имеем (as-is)

### Общая структура
- Rust workspace: `crates/{cli-harness, conf, tools, agent-loop, interactions,
  logger, harness-api, auth, web-harness}`.
- `conf` — единственный крейт, который читает `harness.json5` и model-файлы;
  наружу отдаёт типизированные настройки.
- `tools` — реестр инструментов: `fs`, `git`, `terminal`, `user`, `skills`, `roles`.
- `cli-harness` — консольная точка входа и HTTPS-сервер (`--serve`).
- CI/CD: `.github/workflows/ci.yml` уже содержит job `release`, который
  вызывает `softprops/action-gh-release@v2`.

### CWD (текущее поведение)
- Ключ `cwd` — обязательное поле `harness.json5`.
- Относительный `cwd` из конфига резолвится относительно **каталога
  конфига** (`conf_dir`), а не process cwd.
- `--cwd` работает как override значения из конфига и тоже резолвится
  относительно `conf_dir`.
- Относительные пути `tools.fs.*` и `tools.terminal.cwd-list` резолвятся
  относительно `settings.cwd`.
- `agent.config`, `skills.*.file`, `roles.*.file` резолвятся относительно
  `conf_dir`.
- `default_conf_path()` в консольном режиме всегда использует process cwd и
  **не учитывает** `--cwd`; в server-режиме `default_conf_path_for_serve`
  уже учитывает `--cwd`.
- Корневые ключи `harness.json5` не валидируются: неизвестный корневой ключ
  (включая будущий/случайный `cwd`) молча игнорируется.
- `server.tls-cert` / `server.tls-key` читаются как строки без резолва.
- `api-key` в model-файлах: относительный путь к файлу ключа резолвится
  относительно **process cwd**.

### Консольная и server-загрузка
- `conf::load_server_config(conf_path)` вызывается в `server/mod.rs` и в
  **пяти местах** `server/config.rs` (`get_config`, `config_default`,
  `validate_config_text`, `reload_server_config`, `refresh_server_config`,
  плюс косвенно `read_existing_parsed`).
- В `ServerState` / `LoadedServerConfig` эффективный CWD сервера не хранится;
  REST-слой не знает CWD, когда `settings == None`.

### run_shell / request_command_shell_execution
- `run_observed_command` всегда возвращает **весь** stdout+stderr после
  выполнения, одновременно стримя его в родительскую консоль.
- Аргументы `run_shell`: `cwd`, `command`, `arguments` (опциональный).
- Аргументы `request_command_shell_execution`: `reason`, `command`, `cwd`.
- У агента нет способа попросить «только хвост вывода».

### tools.fs deprecated-*
- `deprecated-dirs` — точные строки, матчинг по компонентам абсолютного пути
  (префикс). `deprecated-files` — точный файл.
- Существующие denylist-строки регистронезависимы на Windows через
  `comparable_parts`.
- Нельзя задать «любой каталог `log` на любом уровне вложенности».

### GitHub Actions
- `ci` job собирает и загружает артефакты на 3 ОС. Windows-артефакт сейчас
  называется `harness-<version>.exe`, а целевое имя ассета —
  `harness-<version>-windows.exe`.
- `release` job скачивает артефакты по старым именам и создаёт релиз для
  **любого** тега (`startsWith(github.ref, 'refs/tags/')`).
- Базовая публикация уже реализована; отсутствуют строгая сверка версии,
  release notes и контрольные суммы.

## К чему хотим прийти (to-be)

### CWD first
1. `cwd` полностью исключён из `harness.json5` и из настроек, которые
   сохраняются в dump. Попытка задать `cwd` в конфиге — ошибка загрузки.
2. CWD берётся из:
   - `--cwd <path>` — если передан явно (резолвится от process cwd);
   - process current dir — по умолчанию.
3. CWD выводится в консоль **до начала цикла** (до выбора агента / до
   первой итерации) и в startup-инфо сервера.
4. Все относительные пути в `harness.json5` вычисляются **относительно CWD**,
   а не каталога конфига.
5. Любой путь из `harness.json5` (относительный или абсолютный) после
   нормализации обязан находиться **внутри CWD**; иначе — ошибка загрузки.
   Из правила есть два осознанных исключения:
   - `server.tls-cert` / `server.tls-key`;
   - `api-key`-путь в model-файлах (секрет может лежать вне CWD).
6. `--conf <conf>.json5` в любом случае должен находиться внутри CWD.

### Output buffer
- У `run_shell` и `request_command_shell_execution` появляется последний
  аргумент `output_buff_size` (опциональный, default `-1`).
- Значение `-1` — вернуть весь вывод. `N >= 0` — вернуть последние `N`
  Unicode-символов вывода команды.
- Полный вывод по-прежнему стримится в пользовательскую консоль; ограничение
  касается только строки, возвращаемой агенту.
- Значения `< -1` и нецелые — ошибка tool call.

### Deprecated-dir-patterns
- В `tools.fs` появляется новый ключ `deprecated-dir-patterns` (массив строк).
- Поддерживается строгое подмножество glob: `**/<component>` с `*` внутри
  компонента. Всё, что вне подмножества, — ошибка конфигурации.
- Паттерн `**/log` запрещает любой каталог `log` на любом уровне; файлы
  внутри него также запрещены.
- Проверка применяется во всех fs-операциях, в `request_read/write_access_for`
  и в `list_folder` (обход не заходит в запрещённые каталоги).
- На Windows матчинг регистронезависимый, на Unix — регистрозависимый.

### GitHub release action
- Релиз создаётся автоматически только для тегов `v*`.
- Версия тега (с отрезанным `v`) сверяется с `[workspace.package].version`
  из `Cargo.toml`.
- К релизу прикладываются `harness-<version>-windows.exe`,
  `harness-<version>-linux`, `harness-<version>-macos` и их `.sha256`.
- Имена upload-артефактов и download-ассетов согласованы.
- Release notes: `generate_release_notes: true`.

## Границы и ограничения
- Консультация не меняет код; далее Project Analyst / Senior Developer
  превращают эти концепции в задачи разработки.
- Rust-крейты должны оставаться совместимыми с Windows, Linux и macOS.
- `conf` остаётся единственным владельцем чтения json5-конфигов и
  владельцем трансляции glob-паттернов `deprecated-dir-patterns`.
- Проверка «внутри CWD» — лексическая, по компонентам пути. Симлинки и
  junction'ы не канонизируются: это принятый риск, документация не должна
  обещать «строгую изоляцию» от выхода через symlink.
- Решения принимаются по принципам KISS и YAGNI: не вводим новые внешние
  сервисы или тяжёлые крейты без необходимости.
