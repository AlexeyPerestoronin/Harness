# Report from Refactoring Manager — Phase 7, Consulting 3

Дата: 2026-01-21
Фаза: 7
Консультация: 3
Объект: кодовая база после завершения задач phase-7
(`1-conf-cwd-first-and-glob-validation`, `2-tools-output-buffer-and-deprecated-patterns`,
`3-cli-harness-cwd-first-and-dump-resume`, `4-github-release-action`).

---

## 1. Резюме

Кодовая база после phase-7 находится в хорошем состоянии:

- `cargo check --workspace` — успешно, без ошибок.
- `cargo test --workspace` — успешно: 42 теста проходят
  (`agent-loop` — 5, `cli-harness` — 8, `conf` — 13, `tools` — 16).
- Публичные API и CLI-контракты, зафиксированные в consulting 3, в целом
  реализованы корректно: CWD-first, `deprecated-dir-patterns`,
  `output_buff_size`, dump с `"CWD"`, release action для `v*`.

Основной долг — **документация** и **накопленное дублирование**, а не
логические дефекты:

1. `README.md`, `tools/help-user.txt`, `REST-API.help.md` и `openapi.yaml`
   содержат заметные расхождения с фактическим поведением phase-7
   (остались упоминания `cwd` в конфиге, старые имена артефактов, старые
   правила резолва путей и т.п.).
2. В коде есть несколько точек дублирования (`XaiAgent` против
   `OpenAiCompatAgent`, повторная валидация fs-путей, повторная
   read/parse-цепочка в `conf`, несколько реализаций лексической
   нормализации путей).
3. `cargo clippy --workspace --all-targets` даёт 7 предупреждений, все —
   низкого риска, но их стоит закрыть для чистоты CI.

Блокирующих проблем (ошибок компиляции, проваленных тестов, критических
нарушений SOLID/DRY с риском регрессий) не обнаружено.

---

## 2. Методика и выполненная проверка

Проверено вручную:

- `cargo check --workspace`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets`
- чтение кода всех native-крейтов: `conf`, `tools`, `cli-harness`,
  `agent-loop`, `auth`, `harness-api`, `interactions`, `logger`;
- сверка с `README.md`, `crates/cli-harness/help.txt`,
  `crates/tools/help-user.txt`, `api/v1/**`, `.github/workflows/ci.yml`,
  `Cargo.toml`.

Фактические результаты команд приведены в Приложении A.

---

## 3. Оценка по обязательным критериям

### 3.1. Структура кодовой базы

Хорошо. Workspace разделён по ответственности:

- `conf` — единственный владелец чтения json5 и трансляции glob;
- `tools` — реестр инструментов;
- `agent-loop` — цикл, агенты, vendor-request, dump;
- `cli-harness` — CLI и HTTP-сервер;
- `harness-api` — DTO без зависимостей от axum/leptos;
- `auth`, `interactions`, `logger` — маленькие выделенные крейты.

Замечания по структуре связаны только с дублированием внутри крейтов
(см. 4.4–4.6).

### 3.2. Производительность

Критических проблем нет.

- Обрезка `output_buff_size` реализована как `tail_chars` без полного
  `Vec<char>`, как и задумано.
- `deprecated-dir-patterns` компилируются один раз на `Sandbox`, поиск
  `is_match` дёшев, запрещённые поддеревья не обходятся.
- Единственное замечание: паттерны фактически компилируются **дважды** —
  в `conf::load_fs` (для валидации) и в `Sandbox::from_fs` (для матчинга).
  При единицах паттернов это некритично; см. рекомендацию 4.7.
- `ServerConfigState` имеет `large_enum_variant` (Clippy): вариант `Ok`
  содержит `ServerSettings` (~648 байт), из-за чего весь enum передаётся
  по значению крупным. В текущем объёме вызовов это не горячая точка,
  но стоит осознанно закрыть (см. 4.3).

### 3.3. Сопровождаемость

Умеренно хорошая. Основные риски для сопровождения:

- документация разошлась с кодом;
- `XaiAgent` копирует большую часть `OpenAiCompatAgent`;
- несколько параллельных реализаций нормализации путей;
- отсутствие интеграционных тестов серверного режима.

### 3.4. SOLID / KISS / YAGNI / DRY

- **SOLID** — в целом соблюдается. `conf` знает только про конфигурацию,
  `tools` — только про инструменты, агенты — только про vendor-протокол.
- **KISS / YAGNI** — соблюдаются. Избыточных абстракций не добавлено.
- **DRY** — есть нарушения, перечисленные в 4.4 и 4.5.

### 3.5. Один модуль — одна ответственность

В основном соблюдается. Смешения, которые стоит отметить:

- `tools::terminal::resolve_from` содержит собственную нормализацию пути,
  которая дублирует `tools::fs::access::abspath` и `conf::fs_scope`.
- `cli-harness/src/server/fs_api.rs` намеренно использует отдельную
  модель проверки путей (каноникализация, deny-list), отличную от
  `tools::fs::Sandbox`. Это осознанно, но различие нужно явно закрепить
  комментарием/документацией, чтобы не выглядело дублированием.

### 3.6. Работа с зависимостями

Проверены все `Cargo.toml`:

- `conf` — `serde_json`, `json5`, `regex` — минимально;
- `tools` — `conf`, `interactions`, `tokio`, `serde_json`, `regex`,
  `encoding_rs` — всё используется;
- `cli-harness` — все зависимости используются;
- `agent-loop` — все зависимости используются;
- `auth`, `harness-api`, `interactions`, `logger` — минимальны.

Висячих зависимостей не найдено. `web-harness` вынесен в собственный
workspace и исключён из корневого — это корректно для Trunk-сборки.

### 3.7. Обратная совместимость

Обратная совместимость проектом не требуется. Кода, обслуживающего только
совместимость со старыми конфигами/dump, не найдено: `conf_dir_of` удалён,
`cwd` в json5 отвергается, старые dump не поддерживаются. Замечаний нет.

### 3.8. Качество архитектуры

Архитектура CWD-first реализована целостно:

- `ensure_conf_in_cwd` вызывается до чтения файла;
- `resolve_in_cwd` — единая точка проверки «внутри CWD» для конфига;
- осознанные исключения (`tls-cert`/`tls-key`, `api-key`,
  `git.executable`) сохранены и задокументированы.

Рекомендации по снижению связности см. 4.5 (общая path-утилита) и 4.6
(единая read/parse-цепочка в `conf`).

---

## 4. Найденные проблемы и рекомендации

Приоритеты: **P1** — обязательно исправить в ближайшей итерации,
**P2** — желательно, **P3** — можно отложить/принять осознанно.

### P1-1. README.md расходится с поведением phase-7

**Где:** `README.md`.

Расхождения:

1. Блок CLI:
   ```text
   harness.exe [--cwd <path>] [--api-key <key>] --dump "<path to dump file>"
   ```
   Фактически `--cwd` с `--dump` запрещён (CLI-ошибка). Должно быть:
   ```text
   harness.exe [--api-key <key>] --dump "<path to dump file>"
   ```
2. Строка про release-артефакты:
   > `harness-<version>-linux`, `harness-<version>-macos` and `harness-<version>.exe`
   Фактически Windows-артефакт называется `harness-<version>-windows.exe`.
3. Правило для `--conf`:
   > Default: `<process-cwd>/harness.json5` for console sessions and
   > `<cwd>/harness.json5` for the server.
   Фактически в обоих режимах default — `<cwd>/harness.json5`.
   Относительный `--conf` резолвится от process cwd, затем обязан быть
   внутри итогового CWD.
4. Раздел Configuration:
   - ключ `cwd` описан как поле `harness.json5`, хотя теперь он
     отвергается; CWD задаётся только `--cwd` или process cwd;
   - `agent.config`: «Relative paths are resolved against the directory of
     this conf file» — теперь от CWD и с проверкой «внутри CWD».
5. Раздел Tools для `fs` не упоминает `deprecated-dir-patterns`.
6. Раздел CI:
   - «uploads `harness-<version>.exe`, `harness-<version>-linux` and
     `harness-<version>-macos` artifacts» — должно быть
     `harness-<version>-windows.exe`;
   - «creates a GitHub Release with the three native binaries when a tag
     is pushed» — фактически только для тегов `v*`, с проверкой версии из
     `Cargo.toml` и с `.sha256`.

**Рекомендация:** привести `README.md` в соответствие с `help.txt` и
`ci.yml`; при описании CWD дать явные формулировки из consulting 3.

---

### P1-2. `tools/help-user.txt` содержит устаревшие/противоречивые сведения

**Где:** `crates/tools/help-user.txt`.

1. Раздел «Variables» содержит пример:
   ```json5
   "cwd": "${vars:work-dir}",
   ```
   Это теперь ошибка загрузки: корневой ключ `cwd` отвергается.
   Пример нужно убрать или заменить комментарием, что CWD задаёт запуск.
2. Разделы `skills` и `roles`:
   > Relative paths are resolved against the directory of harness.json5.
   Фактически — от CWD, с проверкой «внутри CWD» (см. `conf::load_named_files`).
3. Раздел «Access model»:
   > Relative paths in these lists are resolved against the current working
   > directory of the process, not settings.cwd.
   Фактически относительные `available-dirs` / `available-files` /
   `deprecated-*` резолвятся от `settings.cwd`/CWD и должны оставаться
   внутри CWD. Эта строка противоречит и новой модели, и поведению
   `Sandbox::resolve_in_root`.

**Рекомендация:** обновить указанные разделы; для fs-списков явно писать
«относительные пути резолвятся от CWD и обязаны быть внутри CWD».

---

### P1-3. REST-API документация разошлась с phase-7

**Где:** `api/v1/REST-API.help.md`, `api/v1/openapi.yaml`.

`REST-API.help.md`:

1. Пример ошибки `PUT /api/v1/config` содержит
   `"message": "missing 'cwd'"`. После phase-7 `cwd` в конфиге запрещён,
   а сообщение — про использование `--cwd`/process cwd. Пример вводит в
   заблуждение.
2. Примеры `GET /api/v1/config` и `PUT /api/v1/config` содержат
   `"cwd": "C:/MyLife/Harness"` в теле конфига. Такого ключа больше нет.
3. Мелкая опечатка в начале: «any HTTP lient that supports HTTPS».

`openapi.yaml`:

4. `info.version: 1.5.0` не совпадает с workspace-версией `2.1.0`.
   Если версия OpenAPI отслеживает версию сервера — обновить.
5. `servers[].variables.port.default: '443'`, тогда как фактический
   default-порт сервера — `4067` (`--port` → `server.port` → `4067`).

**Рекомендация:** обновить примеры и метаданные. DTO/схемы (`schemas.yaml`)
менять не требуется — расхождений в самих DTO не найдено.

---

### P2-1. Предупреждения Clippy

**Где:** по всем крейтам; список фактических предупреждений из
`cargo clippy --workspace --all-targets`:

| Крейт | Место | Предупреждение | Рекомендация |
|---|---|---|---|
| `interactions` | `src/cli.rs:113` | `manual_pattern_char_comparison` | `trim_end_matches(['\r', '\n'])` |
| `conf` | `src/types.rs:254` | `large_enum_variant` для `ServerConfigState` | см. P2-2 |
| `tools` | `src/terminal.rs:121` | `needless_borrows_for_generic_args` | убрать `&` у `resolve_from(...)` |
| `tools` | `src/lib.rs:219` | `map_identity` | убрать `.map_err(|e| e)?` |
| `agent-loop` | `src/vendor_request.rs:531` | `redundant_closure` | `PipelineError::Transport` вместо замыкания |
| `cli-harness` | `src/cli.rs:215` | `collapsible_if` | объединить вложенные `if` |
| `cli-harness` | `src/server/mod.rs:150` | `unnecessary_lazy_evaluations` | `unwrap_or(tls_paths.cert.as_path())` |

**Рекомендация:** закрыть все, кроме, возможно, `large_enum_variant`
(по решению по P2-2), и добавить `cargo clippy --workspace --all-targets`
в CI (или хотя бы `-D warnings` для native-крейтов), чтобы дрейф не
накапливался.

---

### P2-2. `ServerConfigState` — крупный enum-вариант

**Где:** `crates/conf/src/types.rs`, enum `ServerConfigState`.

`Ok(ServerSettings)` весит ~648 байт, тогда как `Missing` — 0, `Invalid` —
24. Enum целиком возвращается по значению из `load_server_config`.

**Варианты:**

1. `Ok(Box<ServerSettings>)` — уменьшает размер enum; меняет публичный API
   `conf` и все pattern-match в `cli-harness`.
2. Оставить как есть и добавить
   `#[allow(clippy::large_enum_variant)]` с комментарием «вызовы редки,
   горячей точки нет».

**Рекомендация:** выбрать вариант 2 на текущей итерации (минимально
инвазивно, API не меняется). Вариант 1 закладывать только при следующем
крупном изменении `conf`, если `load_server_config` станет горячим.

---

### P2-3. `AgentLoop::new` принимает неиспользуемый `_log_folder`

**Где:** `crates/agent-loop/src/agent_loop.rs`.

Сигнатура `new(cwd, _log_folder, agent, tools, interaction, session_dir)`
принимает `_log_folder`, но внутри не использует. Вызывающие
(`main.rs`, `server/run.rs`) уже вычисляют `session_dir` через
`session_dir(&cwd, &log_folder, ...)` до вызова.

**Рекомендация:** убрать параметр `_log_folder` из `AgentLoop::new` и из
двух call-site'ов. Это упрощает контракт и снимает мёртвый аргумент.

---

### P2-4. Дублирование веток создания шаблона в `config_default`

**Где:** `crates/cli-harness/src/server/config.rs`.

В `config_default` блок `if !path.exists()` и ветка
`Ok(ServerConfigState::Missing)` делают одно и то же: записывают шаблон,
вызывают `refresh_server_config`, возвращают `ConfigDefaultResponse`.
Вторая ветка существует для гонки «файл исчез между проверками», но
тело дублировано.

**Рекомендация:** вынести общий `create_default_config_response`
(или аналогичный) и переиспользовать в обеих ветках.

---

### P2-5. Дублирование read/parse/vars/root-validation в `conf::load`

**Где:** `crates/conf/src/load.rs`.

`load_with` и `load_server_config` одинаково делают:
`normalize_abs` → `ensure_conf_in_cwd` → `read_to_string` →
`vars::substitute` → `json5::from_str` → `expect_object` →
`validate_root_keys`. Различается только обработка ошибок
(`Err` против `ServerConfigState::Invalid`).

**Рекомендация:** выделить внутреннюю
`read_and_parse(conf_path) -> Result<Value, ConfError>` или
`read_root(conf_path) -> Result<Value, ConfError>`, а затем в
`load_with` и `load_server_config` оставить только ветвление результата.
Это уменьшит дрейф порядка проверок.

---

### P3-1. `XaiAgent` дублирует `OpenAiCompatAgent`

**Где:**
- `crates/agent-loop/src/agents/xai.rs`
- `crates/agent-loop/src/agents/openai_compat.rs`

`XaiAgent` содержит собственную реализацию `iteration`, `handle_tool_calls`,
`finish`, учёта usage и dump-структуры, почти полностью совпадающую с
`OpenAiCompatAgent`. Реальные отличия xAI:

- URL;
- заголовки `x-grok-conv-id` и `conversation_id`;
- поля в теле (`conversation_id`);
- парсер `cached_prompt_text_tokens` и логика `conversation_id` в dump;
- cost `cost_in_usd_ticks` (уже поддержан общим `real_cost_from_body`).

**Рекомендация:** расширить `OpenAiCompatAgent` точками расширения
(extra headers, extra body fields, vendor parser/cache mapping) и сделать
`XaiAgent` обёрткой, как уже сделано для `DeepSeekAgent` и
`AlibabaAgent`. Это устранит самое заметное DRY-нарушение в
`agent-loop`.

---

### P3-2. Дублирование `finish()` / логирования статистики в агентах

**Где:** `xai.rs`, `openai_compat.rs`, `claude.rs` (и частично `google.rs`).

Блок «Sessions statistic» (`total requests`, `total tokens`,
`input/output/cached`, `USD approximated`, `USD real`) повторяется в
трёх файлах.

**Рекомендация:** вынести общий
`log_usage_summary(interaction, &UsageStats, consumed_tokens)` в
`agents/mod.rs` и переиспользовать. Google-версия может остаться
отдельной из-за другого формата строк.

---

### P3-3. Несколько реализаций лексической нормализации путей

**Где:**

- `crates/conf/src/fs_scope.rs` — `normalize_abs`, `normalize_lexical`,
  `comparable_parts`;
- `crates/cli-harness/src/session.rs` — `normalize_abs`;
- `crates/tools/src/fs/access.rs` — `abspath`, `resolve_in_root`,
  `normalize_root`, `normalize_lexical`, `path_starts_with`,
  `comparable_parts`;
- `crates/tools/src/terminal.rs` — `resolve_from` (своя вариация).

Семантика почти одинаковая, но реализации независимы; любое будущее
изменение (например, обработка `..` выше корня или Windows-prefix) придётся
вносить в несколько мест, что уже даёт мелкие различия
(в `conf::comparable_parts` RootDir/Prefix обрабатываются иначе, чем в
`tools::fs::access::comparable_parts`).

**Рекомендация:** ввести одну общую path-утилиту (например, в `conf` как
`pub mod pathutil` или отдельный маленький crate) и использовать её в
`conf`, `tools` и `cli-harness`. Сейчас это не блокер, но снижает
сопровождаемость.

---

### P3-4. Двойная проверка «внутри CWD» для `tools.fs`

**Где:** `crates/conf/src/load.rs` (`load_fs`) и
`crates/conf/src/fs_scope.rs` (`check_conflicts`).

`load_fs` сначала сам валидирует каждый allow/deny-путь через
`resolve_in_cwd`, затем `check_conflicts` повторно резолвит те же пути
через `resolve_raw`. Это задокументировано, но остаётся дублированием
работы и создаёт два места, где сообщение об ошибке может разъехаться.

**Рекомендация:** определить одну точку ответственности: либо `load_fs`
возвращает уже нормализованные пути, либо `check_conflicts` является
единственным местом валидации. Минимальный вариант — оставить, но
зафиксировать комментарием и тестом одинаковый текст ошибок.

---

### P3-5. Нестрогая валидация целых чисел в session-полях конфига

**Где:** `crates/conf/src/load.rs`.

`required_i64` и `required_u64` используют
`as_i64`/`as_u64`, которые принимают JSON-число с дробной частью и молча
усекают его через `as f64 as i64`/`as f64 as u64`.

Например, `"token-limit": 128.7` в `harness.json5` будет принят как `128`.
Для model-файлов это уже сделано строго (`strict_i64` в
`required_model_int`), а для session-полей `agent` и `tools.*` — нет.

**Рекомендация:** перевести `required_i64`/`required_u64` (session-лимиты,
execution-limit и т.п.) на строгие `strict_i64`/`strict_u64`.
Это ужесточает валидацию только для заведомо некорректных конфигов; с точки
зрения рефакторинга это «поведенческое» уточнение, поэтому перед
применением согласовать с Project Owner/Project Manager.

---

### P3-6. `arg_str` принимает bool и молча приводит его к строке

**Где:** `crates/tools/src/lib.rs`.

`arg_str` для `Value::Bool(b)` возвращает `Ok(b.to_string())`. Для
строковых аргументов-путей это превращает ошибочный tool-call в
последующую ошибку файловой системы, менее понятную агенту.

**Рекомендация:** для строковых аргументов либо отклонять `bool`
(предпочтительно для `path`/`command`), либо оставить только для
аргументов, где это осмысленно (например, `depth`). Уточнить на уровне
контракта tools и покрыть тестами.

---

### P3-7. Отсутствуют интеграционные тесты серверного режима

**Где:** `crates/cli-harness/src/server/*`.

Phase-7 `api.md` перечисляет серверные сценарии (валидность конфига без
`cwd`, `cwd` в конфиге → `CONFIG_INVALID`, `--conf` вне CWD, `PUT` с
`git.executable: "git"` и т.п.), но в `cli-harness` есть только unit-тесты
`cli.rs` и `session.rs`. Тестов `server/config.rs`, `handlers.rs`,
`fs_api.rs` нет.

**Рекомендация:** добавить unit-тесты для `validate_config_text`,
`config_default` и `get_config` (хотя бы через временный каталог и
`LoadedServerConfig`), а также, по возможности, один-два
HTTP-теста через axum `tower::ServiceExt`. Это закроет наиболее
регрессионно-опасные места CWD-first.

---

### P3-8. Тест dump-формата не использует реальный `source` из `conf`

**Где:** `crates/agent-loop/src/dump_settings.rs`.

Тест `embed_writes_settings_and_cwd_and_extract_reads_them_back` подаёт
руками собранный `settings` без `cwd`. Проверки того, что реальный
`Settings::source` после `conf::load*` не содержит `cwd`, нет на уровне
агентского цикла.

**Рекомендация:** добавить в `conf`/`agent-loop` тест: загрузить
валидный конфиг → взять `settings.source` → `embed` → прочитать dump и
убедиться, что `settings` внутри dump не содержит `cwd`, а `"CWD"` —
абсолютный и совпадает с `settings.cwd`.

---

## 5. Итоговая оценка и план

Состояние кодовой базы — **удовлетворительное/хорошее**; phase-7
реализован по утверждённой архитектуре, тесты проходят.

Рекомендуемый порядок следующей итерации:

1. **P1** — синхронизировать документацию: `README.md`,
   `tools/help-user.txt`, `REST-API.help.md`, `openapi.yaml`.
2. **P2** — закрыть clippy-предупреждения (кроме `large_enum_variant`
   по решению), убрать `_log_folder`, устранить дублирование в
   `config_default` и `conf::load`.
3. **P3** — рефакторинг агентов (`XaiAgent` → общий compat-слой),
   общая path-утилита, усиление тестов серверного режима.
4. Включить clippy в CI с `-D warnings` после закрытия всех
   предупреждений, чтобы дрейф не накапливался.

---

## Приложение A. Фактические результаты проверок

### A.1. `cargo check --workspace`

```text
    Checking tools v2.1.0
    Checking agent-loop v2.1.0
    Checking cli-harness v2.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.18s
```

### A.2. `cargo test --workspace`

```text
running 5 tests   (agent-loop)  — ok
running 8 tests   (cli-harness) — ok
running 13 tests  (conf)        — ok
running 16 tests  (tools)       — ok
```

Итого: **42 теста прошли**, 0 failed.

### A.3. `cargo clippy --workspace --all-targets`

Выявлено 7 предупреждений:

- `interactions`: `manual_pattern_char_comparison` (`src/cli.rs:113`);
- `conf`: `large_enum_variant` (`src/types.rs:254`);
- `tools`: `needless_borrows_for_generic_args` (`src/terminal.rs:121`),
  `map_identity` (`src/lib.rs:219`);
- `agent-loop`: `redundant_closure` (`src/vendor_request.rs:531`);
- `cli-harness`: `collapsible_if` (`src/cli.rs:215`),
  `unnecessary_lazy_evaluations` (`src/server/mod.rs:150`).
