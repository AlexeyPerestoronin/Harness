# Задача на разработку: CWD first и glob-валидация deprecated-dir-patterns в крейте conf

Доработать крейт `crates/conf` в соответствии с утверждённым планом phase-7 (consulting 3): реализовать модель CWD first и добавить поддержку `tools.fs.deprecated-dir-patterns` со строгим glob-подмножеством `**/<component>` и валидацией/компиляцией паттернов в regex. Изменения ограничиваются крейтом `conf` и его шаблоном конфигурации.

Зачем: чтобы `cli-harness` мог запускаться как системная утилита, у которой рабочая директория задаётся внешним контекстом запуска, а не полем конфига; и чтобы пользователь мог гибко запрещать fs-доступ по glob-паттернам вида `**/log`, `**/done`, `**/note-*`.

Ожидаемый результат: крейт `conf` компилируется, unit-тесты `cargo test -p conf` проходят, публичные API и шаблон конфигурации соответствуют плану phase-7.

## Функциональные требования

### CWD first

1. `cwd` полностью исключается из `harness.json5`: `Settings.cwd` и `ServerSettings.cwd` остаются рантайм-полями, но больше не читаются из json5; их заполняет вызывающий код.
2. `Settings.conf_dir` / `ServerSettings.conf_dir` остаются только как информационный путь конфига и больше не используются для резолва относительных путей.
3. В `str/keys.rs`:
   - `pub const CWD: &str = "cwd";` остаётся, но документируется как отвергаемый корневой ключ: CWD задаётся только через `--cwd` или process cwd;
   - добавляется `pub const DEPRECATED_DIR_PATTERNS: &str = "deprecated-dir-patterns";`;
   - добавляется константа со списком допустимых корневых ключей (`vars`, `server`, `agent`, `tools`) для корневой валидации.
4. В `load.rs` вводятся новые сигнатуры:
   - `load(conf_path, cwd) -> Result<Settings, ConfError>`
   - `load_with(conf_path, cwd, chooser) -> Result<Settings, ConfError>`
   - `load_from_value(root, cwd, chooser) -> Result<Settings, ConfError>`
   - `load_server_config(conf_path, cwd) -> Result<ServerConfigState, ConfError>`
   - `cwd` всегда передаётся извне; `cwd_override` и чтение `cwd` из json5 удаляются.
5. `load` / `load_with` / `load_server_config` первым делом приводят `conf_path` к абсолютному нормализованному виду и вызывают `ensure_conf_in_cwd(conf_path, cwd)`: конфиг обязан находиться внутри CWD, иначе возвращается `Err(ConfError)`.
6. Порядок проверок в `load_server_config`: сначала `ensure_conf_in_cwd`, затем чтение/парсинг файла. Поэтому `--conf` вне CWD при отсутствующем файле — это ошибка запуска, а не `ServerConfigState::Missing`.
7. Добавляется общий помощник `resolve_in_cwd(raw, cwd) -> Result<PathBuf, ConfError>`:
   - относительный путь → `cwd.join(raw)`;
   - абсолютный путь → оставляется как есть;
   - результат нормализуется (`.` / `..`);
   - проверяется, что результат находится внутри `cwd` по компонентам пути (на Windows — регистронезависимо); иначе `ConfError`.
8. Через `resolve_in_cwd` проводятся все относительные пути конфига:
   - `agent.config`;
   - `tools.fs.available-dirs` / `available-files` / `deprecated-dirs` / `deprecated-files` (для allow/deny-строк — после снятия префикса `r:`/`w:`);
   - `tools.terminal.cwd-list`;
   - `tools.skills.<name>.file` и `tools.roles.<name>.file`;
   - `agent.log-folder`.
9. Осознанные исключения из правила «внутри CWD»:
   - `server.tls-cert` / `server.tls-key`: относительный путь резолвится от CWD, абсолютный используется как есть; проверка «внутри CWD» не применяется;
   - `api-key` в model-файлах: относительный путь к файлу ключа резолвится от CWD, абсолютный используется как есть; проверка «внутри CWD» не применяется; поведение «если по пути есть файл — читаем его, иначе считаем значение литеральным ключом» сохраняется;
   - `tools.git.executable` не проходит через `resolve_in_cwd`: это внешняя команда/исполняемый файл, значение передаётся в `Command::new` как есть.
10. `load_tools(obj, cwd)` больше не принимает `conf_dir`; `load_skills`, `load_roles` и `load_named_files` резолвят файлы от `cwd`.
11. `load_terminal(obj, cwd)` для каждого элемента `cwd-list` вызывает `resolve_in_cwd` и сохраняет абсолютный нормализованный путь.
12. `load_fs(obj, cwd)` после снятия префиксов `r:`/`w:` проверяет каждый путь через `resolve_in_cwd`.
13. `log_folder_from_agent` остаётся строкой, но её значение проверяется через `resolve_in_cwd(log_folder, cwd)`.
14. `server_settings_from_value(root, cwd)` использует переданный `cwd` и вызывает `resolve_in_cwd` для `agent.config` и `tools`.
15. Резолв `api-key` в model-файлах: `cwd` прокидывается в `load_agent`, `settings_from_model`, `resolve_api_key` и `resolve_api_key_value`; относительный путь резолвится от CWD, абсолютный используется как есть.
16. `resolve_agent_for_run(settings, ...)` использует `settings.cwd` для резолва `api-key` в server-режиме.
17. Публичная `pub fn load_model(model_path, mode)` удаляется; из `lib.rs` убирается её re-export. Внутренние функции `settings_from_model` / `settings_from_model_for_run` остаются, но получают CWD для резолва `api-key`.

### Корневая валидация `harness.json5`

18. Допустимые корневые ключи: `vars`, `server`, `agent`, `tools`.
19. Неизвестный корневой ключ — ошибка загрузки.
20. Для ключа `cwd` — отдельное сообщение об ошибке: `cwd` больше не задаётся в конфиге; используйте `--cwd` или запустите процесс из целевой директории.

### `deprecated-dir-patterns` и glob

21. `FsSettings` получает поле `deprecated_dir_patterns: Vec<String>`.
22. Новый ключ `tools.fs.deprecated-dir-patterns` — массив строк, опциональный, default `[]`.
23. Новый модуль `glob.rs` — единственный владелец трансляции glob → regex:
    - публичная функция `regex_source(pattern) -> Result<String, ConfError>`;
    - нормализует слэши (`\` → `/`);
    - на Windows приводит паттерн к нижнему регистру;
    - проверяет и снимает префикс `**/`;
    - проверяет, что остался ровно один непустой компонент без `/` и без `**`;
    - внутри компонента единственный wildcard — `*` (ноль и более символов, кроме `/`); `*` → `[^/]*`;
    - символы `?`, `[`, `]` и прочие regex-метасимволы трактуются как литералы (экранируются через `regex::escape`);
    - итоговый regex: `(?:^|/)` + компонентный regex + `(?:/|$)`.
24. Примеры трансляции:
    - `**/log` → `(?:^|/)log(?:/|$)`;
    - `**/done` → `(?:^|/)done(?:/|$)`;
    - `**/note-*` → `(?:^|/)note-[^/]*(?:/|$)`;
    - `**/log?` → `(?:^|/)log\?(?:/|$)`.
25. Недопустимые паттерны: `log/**`, `**/log/**`, `foo/**/bar`, `**`, `**/log/`, `**/log/**/x` — ошибка валидации.
26. Сообщение об ошибке `regex_source` содержит: сам паттерн, позицию/причину (`unexpected '/' at position N`, `component must not contain '**'`, `component must not be empty`) и справку: `expected glob pattern '**/<component>'`, `only '*' is a wildcard inside <component>`, `other characters are matched literally`, `regex is not supported`.
27. `load_fs` для каждой строки `deprecated-dir-patterns` вызывает `glob::regex_source` и `Regex::new`; ошибка — ошибка загрузки конфига.
28. В `fs_scope.rs`:
    - `check_conflicts` перед сравнением проверяет, что каждый allow/deny-элемент после нормализации находится внутри `cwd`; выход за пределы — ошибка загрузки;
    - для `deprecated-dir-patterns` лексический conflict-анализ не выполняется: конфликт с `available-*` проявится в рантайме как отказ доступа.
29. В `lib.rs` добавляется `pub mod glob`.

### Шаблон и зависимости

30. В `template.rs` и `settings-template.json5`:
    - убрать поле `cwd` и `vars.workspace`;
    - задать `vars.agent-config` по умолчанию равным `harness/agents/grok-4.6.json5` (существующий файл в репозитории), чтобы созданный из шаблона конфиг был валиден при CWD = корень проекта;
    - обновить комментарии: «относительные пути вычисляются относительно CWD и не могут выходить за его пределы»;
    - добавить `deprecated-dir-patterns: []` с примерами `**/log`, `**/done`, `**/note-*`;
    - в комментариях и тексте шаблона для `deprecated-dir-patterns` использовать термин «glob pattern», а не «regex»; явно указать: поддерживается только wildcard `*`, regex не поддерживается;
    - для `tools.git.executable` зафиксировать: «path or name of the git binary; значение передаётся в `Command::new` и не резолвится от CWD».
31. В `conf/Cargo.toml` добавляется зависимость `regex = "1"`.

## Проверка корректности решения

- `cargo test -p conf` — все тесты крейта проходят.
- `cargo build -p conf` — крейт собирается без ошибок.
- Добавить unit-тесты в `crates/conf`:
  - `harness.json5` без `cwd` валиден;
  - `harness.json5` с `cwd` → ошибка загрузки с отдельным сообщением про `--cwd`/process cwd;
  - неизвестный корневой ключ → ошибка;
  - `resolve_in_cwd`: `.` / `..` нормализуются; путь вне CWD (`../outside`, абсолютный вне CWD) → ошибка; на Windows сравнение компонентов регистронезависимое, а `C:\work` не считается внутри `C:\work2`;
  - `load_server_config`: `--conf` вне CWD при отсутствующем файле → `Err`, а не `Missing` (порядок `ensure_conf_in_cwd` до чтения файла);
  - `regex_source`: `**/log`, `**/done`, `**/note-*`, `**/log?` компилируются в ожидаемые regex; `log/**`, `**/log/**`, `foo/**/bar`, `**`, `**/log/` → ошибка с позицией и справкой;
  - `load_fs` принимает `deprecated-dir-patterns` и отвергает невалидные паттерны как ошибку загрузки;
  - шаблон `settings-template.json5` не содержит `cwd` и содержит `deprecated-dir-patterns`.

## Scope

Минимально необходимые изменения:

- `crates/conf/src/types.rs`
- `crates/conf/src/load.rs`
- `crates/conf/src/fs_scope.rs`
- `crates/conf/src/glob.rs` (новый файл)
- `crates/conf/src/str/keys.rs`
- `crates/conf/src/lib.rs`
- `crates/conf/src/template.rs`
- `crates/conf/settings-template.json5`
- `crates/conf/Cargo.toml`
- `Cargo.lock` — только если cargo обновит его после добавления `regex` в `conf`

Справочные материалы для чтения:

- `WiKi/todo/phase-7/pm-consulting report 3/base.md`
- `WiKi/todo/phase-7/pm-consulting report 3/arch.md`
- `WiKi/todo/phase-7/pm-consulting report 3/tech.md`
- текущие исходники `crates/conf`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 1-conf-cwd-first-and-glob-validation → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
