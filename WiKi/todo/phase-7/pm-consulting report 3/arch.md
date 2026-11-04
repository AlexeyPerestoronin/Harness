# Consulting 3 — Phase 7. Архитектура решения

## 1. Контекстная схема (C4 Context)

```mermaid
flowchart LR
    User[Пользователь / Project Owner]
    Agent[Внешняя LLM API]
    CLI[harness.exe<br/>cli-harness]
    GH[GitHub Actions]
    Releases[GitHub Releases]

    User -->|"--cwd, --conf, --prompt, --dump"| CLI
    CLI -->|"agent loop + tools"| Agent
    GH -->|"build + test + artifacts"| CLI
    GH -->|"Create GitHub Release"| Releases
    User -->|"скачивает дистрибутивы"| Releases
```

## 2. Контейнерная схема workspace

```mermaid
flowchart TD
    subgraph cli-harness
        CLI[cli.rs / main.rs]
        SESSION[session.rs]
        SERVER[server/]
        STATE[server/state.rs]
        SERVERCFG[server/config.rs]
    end

    subgraph conf
        LOAD[load.rs]
        TYPES[types.rs]
        SCOPE[fs_scope.rs]
        GLOB[glob.rs]
        KEYS[str/keys.rs]
        TPL[template.rs + settings-template.json5]
    end

    subgraph tools
        REG[lib.rs ToolRegistry]
        FS[fs.rs + fs/access.rs Sandbox]
        TERM[terminal.rs]
        USER[user.rs]
        PROC[process.rs]
        GIT[git.rs]
    end

    subgraph agent-loop
        AL[agent_loop.rs]
        DUMP[dump_settings.rs]
    end

    CLI --> LOAD
    SESSION --> LOAD
    SERVER --> LOAD
    SERVERCFG --> LOAD
    SERVER --> STATE
    LOAD --> TYPES
    LOAD --> SCOPE
    LOAD --> GLOB
    LOAD --> KEYS
    AL --> REG
    AL --> DUMP
    REG --> FS
    REG --> TERM
    REG --> USER
    REG --> GIT
    TERM --> PROC
    USER --> PROC
    FS --> GLOB
    CLI --> DUMP
```

## 3. CWD first

### 3.1. Новая модель владения CWD

```mermaid
flowchart TD
    A[argv: --cwd?] -->|да| B[session::resolve_cwd: normalize от process cwd]
    A -->|нет| C[std::env::current_dir]
    B --> D[CWD]
    C --> D
    D --> E[conf::load_server_config conf_path, cwd]
    E --> F{conf внутри CWD?}
    F -->|нет| ERR[Ошибка запуска]
    F -->|да| G{файл conf существует?}
    G -->|нет| MISS[ServerConfigState::Missing]
    G -->|да| H[Парсинг harness.json5 + root validation]
    H --> I{каждый путь внутри CWD?}
    I -->|нет| ERR
    I -->|да| J[Settings / ServerSettings с cwd]
    J --> K[Вывод 'cwd: ...' и 'conf: ...' в консоль]
    K --> L[Agent loop / server]
```

Единственный источник истины для CWD — процесс запуска:
- `--cwd <path>` — нормализуется от process cwd (абсолютный оставляется);
- нет `--cwd` — `std::env::current_dir()`.

Конфиг больше не управляет CWD. Наличие ключа `cwd` в `harness.json5` —
ошибка загрузки с отдельным сообщением.

### 3.2. Изменения по крейтам

#### `crates/conf`

- `types.rs`
  - `Settings.cwd` и `ServerSettings.cwd` остаются рантайм-полями, но больше
    не читаются из json5; их заполняет вызывающий код.
  - `Settings.conf_dir` / `ServerSettings.conf_dir` остаются только как
    информационный путь конфига; для резолва относительных путей они больше
    не используются.
  - `FsSettings` получает `deprecated_dir_patterns: Vec<String>`.
- `str/keys.rs`
  - Добавляется `pub const DEPRECATED_DIR_PATTERNS: &str = "deprecated-dir-patterns";`.
  - `pub const CWD: &str = "cwd";` остаётся, но с новой документацией:
    это отвергаемый корневой ключ; CWD задаётся только через `--cwd` или
    process cwd.
  - При необходимости добавляется константа со списком допустимых корневых
    ключей (`vars`, `server`, `agent`, `tools`) для корневой валидации.
- `load.rs`
  - Новые сигнатуры:
    - `load(conf_path, cwd) -> Result<Settings, ConfError>`
    - `load_with(conf_path, cwd, chooser) -> Result<Settings, ConfError>`
    - `load_from_value(root, cwd, chooser) -> Result<Settings, ConfError>`
    - `load_server_config(conf_path, cwd) -> Result<ServerConfigState, ConfError>`
  - `cwd` — всегда переданное извне значение; `cwd_override` и чтение `cwd`
    из json5 удаляются.
  - `load` / `load_with` / `load_server_config` первым делом:
    - приводят `conf_path` к абсолютному виду;
    - проверяют, что conf находится внутри `cwd` (`ensure_conf_in_cwd`);
    - при выходе за пределы возвращают `Err(ConfError)`.
  - Порядок в `load_server_config`: сначала `ensure_conf_in_cwd`, затем
    `fs::metadata`. Поэтому `--conf` вне CWD при отсутствующем файле — это
    ошибка запуска, а не `ServerConfigState::Missing`.
  - Добавляется общий помощник `resolve_in_cwd(raw, cwd) -> Result<PathBuf, ConfError>`:
    - относительный путь → `cwd.join(raw)`;
    - абсолютный путь → оставить;
    - нормализовать `.` / `..`;
    - проверить, что результат внутри `cwd` по компонентам (Windows
      case-insensitive); иначе — `ConfError`.
  - Все относительные пути конфига проводятся через `resolve_in_cwd`:
    - `agent.config`;
    - `tools.fs.available-dirs` / `available-files` /
      `deprecated-dirs` / `deprecated-files`
      (для allow/deny строк — после снятия `r:`/`w:` префикса);
    - `tools.terminal.cwd-list`;
    - `tools.skills.<name>.file` и `tools.roles.<name>.file`;
    - `agent.log-folder`.
  - `tools.git.executable` **не** проходит через `resolve_in_cwd` (см. 3.4).
  - Корневая валидация `harness.json5`: допустимы только `vars`, `server`,
    `agent`, `tools`. Неизвестный корневой ключ — ошибка; для `cwd` —
    отдельное сообщение «задайте CWD через `--cwd` или запустите процесс из
    целевой директории».
  - `server_settings_from_value(root, cwd)` использует переданный `cwd` и
    вызывает `resolve_in_cwd` для `agent.config` и `tools`.
  - `load_tools(obj, cwd)` больше не принимает `conf_dir`; `load_skills`,
    `load_roles` и `load_named_files` резолвят файлы от `cwd`.
  - `load_terminal(obj, cwd)` для каждого элемента `cwd-list` вызывает
    `resolve_in_cwd` и сохраняет абсолютный нормализованный путь.
  - `load_fs(obj, cwd)` после снятия `r:`/`w:` префиксов проверяет каждый
    путь через `resolve_in_cwd`.
  - `log_folder_from_agent` остаётся строкой, но её значение дополнительно
    проверяется через `resolve_in_cwd(log_folder, cwd)`.
  - `parse_tls_path`: относительный TLS-путь резолвится от `cwd`, но **не**
    проверяется на «внутри CWD» (см. 3.4).
  - Резолв `api-key` в model-файлах: `cwd` прокидывается в `load_agent`,
    `settings_from_model`, `resolve_api_key` и `resolve_api_key_value`.
    Относительный путь к файлу ключа резолвится от CWD, абсолютный
    используется как есть; проверка «внутри CWD» не применяется.
  - `resolve_agent_for_run(settings, ...)` использует `settings.cwd` для
    резолва `api-key` в server-режиме.
  - Удаляется публичная `pub fn load_model(model_path, mode)`; из
    `lib.rs` убирается её re-export.
- `fs_scope.rs`
  - `check_conflicts` перед сравнением проверяет, что каждый allow/deny-элемент
    после нормализации находится внутри `cwd`; выход за пределы — ошибка
    загрузки.
  - Для `deprecated-dir-patterns` лексический conflict-анализ не выполняется:
    конфликт с `available-*` проявится в рантайме как отказ доступа.
- `glob.rs` (новый модуль)
  - Единственный владелец трансляции `deprecated-dir-patterns` glob → regex.
  - Публичная функция `regex_source(pattern) -> Result<String, ConfError>`;
    `tools` использует её для компиляции, `conf` — для валидации при загрузке.
  - Сообщение об ошибке содержит: сам паттерн, позицию/причину
    (`unexpected '/'`, `component must not contain '**'`, пустой компонент)
    и справку о доступном синтаксисе: `**/<component>`, внутри компонента
    wildcard только `*`, остальные символы — литералы.
- `template.rs` + `settings-template.json5`
  - Убрать поле `cwd` и `vars.workspace`.
  - Задать `vars.agent-config` по умолчанию равным
    `harness/agents/grok-4.6.json5` (существующий файл в репозитории),
    чтобы созданный из шаблона конфиг был валиден при CWD = корень проекта.
  - Обновить комментарии: «относительные пути вычисляются относительно CWD
    и не могут выходить за его пределы».
  - Добавить `deprecated-dir-patterns: []` с примерами `**/log`, `**/done`,
    `**/note-*`.
  - В комментариях и тексте шаблона для `deprecated-dir-patterns`
    использовать термин «glob pattern», а не «regex»; явно указать:
    поддерживается только wildcard `*`, regex не поддерживается.
  - Для `tools.git.executable` зафиксировать: «path or name of the git binary;
    значение передаётся в `Command::new` и не резолвится от CWD».

#### `crates/cli-harness`

- `cli.rs`
  - `--cwd` задаёт сам CWD, а не override json5-поля.
  - `default_conf_path(cwd: Option<&Path>)`:
    - нет `--cwd` → `<process-cwd>/harness.json5`;
    - есть `--cwd` → `<CWD>/harness.json5`.
  - Один и тот же `default_conf_path(cwd)` используется и для `Start`, и для
    `Serve`; `default_conf_path_for_serve` удаляется.
  - `--dump` не комбинируется с `--cwd` (CWD берётся из dump); ошибка CLI.
  - Обновить `help.txt`:
    - `--cwd` — «workspace root»;
    - `--conf` — default `<cwd>/harness.json5` в обоих режимах;
    - `--dump` — resume; CWD берётся из `"CWD"` в dump; `--cwd` не требуется.
- `session.rs`
  - `resolve_cwd_override(raw, conf_dir)` заменяется на `resolve_cwd(raw)`:
    - абсолютный → `normalize_abs`;
    - относительный → `current_dir()?.join(raw)` → `normalize_abs`.
  - Добавляется `resolve_abs_path(raw)`: относительный `--conf`/`--dump`
    резолвится от **process cwd** и нормализуется; абсолютный — только
    нормализуется. Используется и для фактического открытия файла, и для
    вывода в консоль.
  - `resolve_api_key_override(cwd, raw)` остаётся: относительный путь
    резолвится от CWD, абсолютный используется как есть.
- `main.rs`
  - В `start_new_session`:
    ```rust
    let cwd = session::resolve_cwd(cwd_override.as_deref())?;
    let conf_path = session::resolve_abs_path(&conf_path)?;
    println!("cwd: {}", cwd.display());
    println!("conf: {}", conf_path.display());
    let state = conf::load_server_config(&conf_path, &cwd)?;
    ```
    Пост-загрузочный блок `if let Some(raw) = cwd { settings.cwd = ... }`
    удаляется.
  - Для `Start` без `--dump`: `conf::load_server_config(&conf_path, &cwd)`.
  - Для `--dump`:
    ```rust
    let dump_path = session::resolve_abs_path(&dump_path)?;
    let (settings_value, cwd) = agent_loop::dump_settings::extract(&dump_path)?;
    println!("cwd: {}", cwd.display());
    println!("settings: from dump");
    let settings = conf::load_from_value(settings_value, &cwd, chooser)?;
    ```
    Сигнатура `start_resume_session` меняется на
    `(dump_path, api_key)` — параметр `cwd` из неё убирается.
    Вызов `agent_loop::dump_settings::conf_dir_of` удаляется.
  - Вывод `cwd`/`conf`/`settings` выполняется до интерактивного выбора агента
    и до первой итерации цикла.
- `server/mod.rs`
  - В `serve` сначала вычислить `cwd = session::resolve_cwd(options.cwd.as_deref())?`,
    затем `conf_path = session::resolve_abs_path(&conf_path)?`, затем
    `conf::load_server_config(&conf_path, &cwd)`.
  - Удалить блок, который после загрузки переопределял `settings.cwd` через
    `resolve_cwd_override`.
  - Передавать `cwd` и абсолютный `conf_path` в `LoadedServerConfig`.
  - Startup info печатает `cwd` из `LoadedServerConfig.cwd`, даже если
    конфиг `Missing` / `Invalid`.
- `server/state.rs`
  - В `LoadedServerConfig` добавляются:
    - `pub cwd: PathBuf` — эффективный CWD сервера;
    - гарантия: `path` и `cwd` — абсолютные нормализованные пути.
- `server/config.rs`
  - Все вызовы `conf::load_server_config` получают вторым аргументом `cwd`
    (шесть call-site'ов):
    - `get_config`;
    - `config_default`;
    - `validate_config_text(text, conf_dir, target_path, cwd)`;
    - `read_existing_parsed(path, cwd)`;
    - `reload_server_config(state, path, cwd)`;
    - `refresh_server_config(state, path, cwd)`.
  - `validate_config_text` прокидывает CWD сервера в загрузку временного файла,
    а не полагается на process cwd.
  - `PUT /api/v1/config` берёт CWD из `state.config.cwd` и передаёт его в
    валидацию и перезагрузку.
  - `config_default` в ветке `Missing` после записи шаблона вызывает
    `refresh_server_config` и возвращает фактическое состояние валидации
    (`valid`), а не константу `true`.

#### `crates/agent-loop`

- `agent_loop.rs`
  - `AgentLoop` получает поле `cwd: PathBuf` (сохраняется в `new`).
  - При каждой записи dump вызывается
    `dump_settings::embed(dump_file, &self.dump_settings, &self.cwd)`.
- `dump_settings.rs`
  - Добавляется `pub const CWD_KEY: &str = "CWD";`.
  - `embed(path, settings, cwd)` записывает верхнеуровневые `"settings"` и
    `"CWD"` (значение `cwd` — абсолютная нормализованная строка).
  - `extract(path) -> Result<(Value, PathBuf), String>` возвращает
    `(settings, cwd)`; при отсутствии `"CWD"` или неабсолютном значении —
    ошибка.
  - Удаляются `conf_dir_of` и все его использования.
  - `settings.source` больше не содержит `cwd`.

### 3.3. Возобновление из dump

```mermaid
sequenceDiagram
    participant U as User
    participant CLI as cli-harness
    participant DUMP as dump_settings
    participant CONF as conf::load_from_value

    U->>CLI: harness --dump <path>
    CLI->>CLI: resolve_abs_path(<path>) от process cwd
    CLI->>DUMP: extract(dump_path)
    DUMP-->>CLI: (settings, CWD)
    CLI->>CLI: print cwd / settings
    CLI->>CONF: load_from_value(settings, CWD, chooser)
    CONF-->>CLI: Settings с cwd = CWD из dump
    CLI->>CLI: agent loop продолжает в CWD из dump
```

- `--dump` — единственный обязательный аргумент resume.
- `--cwd` с `--dump` — ошибка CLI.
- `--conf` с `--dump` по-прежнему запрещён.
- `Settings.conf_dir` для dump-загрузки заполняется значением CWD из dump
  (информационное поле).
- Если CWD из dump не существует или недоступен, запуск завершается ошибкой
  до начала цикла.

### 3.4. Осознанные исключения из правила «все пути внутри CWD»

1. **`server.tls-cert` / `server.tls-key`**
   - Относительный путь резолвится от CWD.
   - Абсолютный путь используется как есть.
   - Проверка «внутри CWD» **не применяется**: сертификаты и ключи часто
     лежат в системных/пользовательских каталогах вне рабочей директории.
2. **`api-key` в model-файлах**
   - Относительный путь к файлу ключа резолвится от CWD.
   - Абсолютный путь используется как есть и может лежать вне CWD.
   - Проверка «внутри CWD» **не применяется**; это секрет, а не рабочий файл.
   - Поведение «если по пути есть файл — читаем его, иначе считаем значение
     литеральным ключом» сохраняется.
3. **`tools.git.executable`**
   - Это исполняемый файл / имя команды, а не рабочий файл конфигурации.
   - Значение передаётся в `Command::new` как есть, без резолва от CWD и без
     проверки «внутри CWD».
   - Относительное значение без разделителей пути (например, `"git"`)
     ищется в `PATH` операционной системы.
   - Исключение фиксируется в `settings-template.json5`, `help-user.txt`
     и тестах.

## 4. Output buffer for shell commands

### 4.1. Поток выполнения

```mermaid
sequenceDiagram
    participant LLM as Agent
    participant REG as ToolRegistry
    participant T as Terminal/User module
    participant P as process::run_observed_command
    participant OS as Shell

    LLM->>REG: run_shell { cwd, command, arguments, output_buff_size }
    REG->>T: call(...)
    T->>P: run_observed_command(cmd, cwd, timeout, output_buff_size)
    P->>OS: spawn + stream stdout/stderr
    OS-->>P: full output
    P->>P: decode + tail(output_buff_size)
    P-->>T: String (обрезанная)
    T-->>LLM: result
```

### 4.2. Изменения
- `tools/src/process.rs`
  - `run_observed_command(cmd, cwd, timeout_secs, output_buff_size)`.
  - После `decode_command_output` применить `tail_chars(s, output_buff_size)`.
  - `tail_chars` реализуется без полного `Vec<char>`:
    `s.chars().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect()`.
  - Стриминг в stdout остаётся полным (для человека).
- `tools/src/lib.rs`
  - Добавить `arg_i64(args, key, default) -> Result<i64, String>`:
    принимает JSON-число или числовую строку; отсутствие/null → `default`;
    отклоняет bool/object/array/дробные/выход за `i64`.
  - Вызывающие модули дополнительно проверяют `-1` или `>= 0`.
- `tools/src/terminal.rs`
  - `run_shell` читает `output_buff_size` через `arg_i64(args, "output_buff_size", -1)?`,
    валидирует диапазон и передаёт в `run_observed_command`.
  - Schema `run_shell`:
    ```json
    {
      "type": "object",
      "properties": {
        "cwd": {"type": "string"},
        "command": {"type": "string"},
        "arguments": {"type": "string"},
        "output_buff_size": {
          "type": "integer",
          "description": "N символов с конца; -1 = весь вывод"
        }
      },
      "required": ["cwd", "command"]
    }
    ```
- `tools/src/user.rs`
  - `request_command_shell_execution` читает `output_buff_size` последним
    аргументом (default `-1`), валидирует и передаёт в `run_observed_command`.
  - Schema обновляется аналогично.
- Семантика `output_buff_size`:
  - `-1` — полный вывод;
  - `0` — пустая строка-результат;
  - `N > 0` — последние `N` Unicode-символов вывода;
  - `< -1` или нецелое — ошибка tool call.
- Маркер обрезки не возвращаем: агент сам задал размер и знает правило.

## 5. Deprecated-dir-patterns (glob)

### 5.1. Модель
- В `tools.fs` добавляется ключ `deprecated-dir-patterns` (массив строк,
  опциональный, default `[]`).
- Поддерживаемое подмножество glob:
  - паттерн обязан иметь вид `**/<component>`;
  - `<component>` — один непустой компонент пути без `/` и без вложенного `**`;
  - внутри компонента единственный wildcard — `*` (ноль и более символов,
    кроме `/`).
- Символы `?`, `[`, `]` и прочие regex-метасимволы **не являются**
  wildcard'ами и трактуются как литералы (экранируются при трансляции).
- Примеры допустимых: `**/log`, `**/done`, `**/note-*`, `**/build-*`,
  `**/log?` (совпадёт с каталогом/файлом `log?`).
- Недопустимые: `log/**`, `**/log/**`, `foo/**/bar`, `**`, `**/log/`,
  `**/log/**/x`.
- Паттерн применяется к нормализованному относительному пути
  (относительно CWD, слэши `/`, без ведущего слэша).
- Для проверяемого пути дополнительно проверяются все ancestor-пути:
  - для файла `a/log/x.md` → `a`, `a/log`, `a/log/x.md`;
  - для директории `a/log` → `a`, `a/log`.

### 5.2. Трансляция glob → regex
- Единственная функция `conf::glob::regex_source(pattern)`:
  - нормализует слэши (`\` → `/`);
  - на Windows приводит паттерн к нижнему регистру;
  - проверяет/снимает префикс `**/`;
  - проверяет, что остался ровно один компонент без `/` и без `**`;
  - в компоненте `*` → `[^/]*`, литералы экранируются `regex::escape`;
  - итог: `(?:^|/)` + компонентный regex + `(?:/|$)`.
- Примеры:
  - `**/log` → `(?:^|/)log(?:/|$)`;
  - `**/done` → `(?:^|/)done(?:/|$)`;
  - `**/note-*` → `(?:^|/)note-[^/]*(?:/|$)`;
  - `**/log?` → `(?:^|/)log\?(?:/|$)`.
- Матчинг идёт по относительному пути, также нормализованному
  (слэши `/`, на Windows — lowercase). На Unix матчинг регистрозависимый.

### 5.3. Валидация при загрузке и ошибки
- `conf::load_fs` для каждой строки `deprecated-dir-patterns` вызывает
  `glob::regex_source` и `Regex::new`; ошибка — ошибка загрузки конфига.
- Сообщение об ошибке содержит:
  - конкретный паттерн;
  - позицию/причину (`unexpected '/' at position N`,
    `component must not contain '**'`, `component must not be empty`);
  - справку: `expected '**/<component>'`, `only '*' is a wildcard`,
    `all other characters are matched literally`.
- Это гарантирует раннюю диагностику как при старте, так и при
  `PUT /api/v1/config` (валидация до применения конфигурации).

### 5.4. Изменения
- `conf`
  - `FsSettings.deprecated_dir_patterns: Vec<String>`.
  - `load_fs` читает новый ключ и валидирует/компилирует паттерны.
  - В `conf/Cargo.toml` добавляется `regex = "1"`.
- `tools`
  - `Sandbox` хранит `Vec<Regex>` для паттернов.
  - `Sandbox::from_fs(root, available_dirs, available_files, extensions,
    deprecated_dirs, deprecated_files, deprecated_dir_patterns)
    -> Result<Self, String>`:
    - компилирует паттерны через `conf::glob::regex_source`;
    - ошибку компиляции возвращает, а не паникует и не игнорирует.
  - `FsModule::build` становится fallible и возвращает
    `Result<(Option<Self>, Arc<Mutex<Sandbox>>), String>`.
  - `ToolRegistry::build` становится fallible и возвращает
    `Result<Self, String>`; `AgentLoop::new` пробрасывает ошибку.
  - Инвариант: в штатном сценарии `conf` уже провалидировал паттерны,
    поэтому `ToolRegistry::build` не падает; fallible-цепочка защищает
    от ручного создания `Sandbox` с невалидными значениями.
  - `is_deprecated_abs` дополнительно проверяет относительный путь и его
    ancestors по скомпилированным паттернам.
  - `walk_folder` продолжает пропускать deprecated-узлы, в том числе по
    паттернам (через `is_deprecated`).
- `tools/help-user.txt` — описать новый ключ, допустимый синтаксис,
  единственный wildcard `*` и примеры; использовать термин
  «glob pattern `**/<component>`» и явно указать, что regex не
  поддерживается.

## 6. GitHub release action

### 6.1. Целевая схема release job

```mermaid
flowchart TD
    PUSH[push master / tag] --> CI[ci job: test + build на 3 OS]
    CI --> ART[upload-artifact windows/linux/macos]
    PUSH --> REL{tag v*?}
    REL -->|да| VER[verify tag == Cargo.toml version]
    VER --> DL[download-artifact 3 OS]
    DL --> ASSETS[prepare release/* + sha256]
    ASSETS --> GHRELEASE[softprops/action-gh-release@v2 generate_release_notes: true]
    GHRELEASE --> RELOUT[GitHub Release]
```

### 6.2. Доработки `ci.yml`
1. **Именование артефактов (upload и download синхронно):**
   - Windows: `harness-<version>-windows.exe`;
   - Linux: `harness-<version>-linux`;
   - macOS: `harness-<version>-macos`.
   - В `ci` job меняется шаг `Artifact name and path` (Windows-имя).
2. **Строгая валидация имени артефакта** в шаге
   «Delete previous artifacts with the same name»:
   ```regex
   ^harness-[0-9]+\.[0-9]+\.[0-9]+(-linux|-macos|\.exe|-windows\.exe)$
   ```
   Допускаются только:
   - `harness-<version>-windows.exe`;
   - `harness-<version>-linux`;
   - `harness-<version>-macos`;
   - legacy `harness-<version>.exe`.
   Отклоняются `harness-<version>-windows`, `harness-<version>-linux.exe`,
   `harness-<version>-macos.exe`, бессуффиксное имя.
3. **Триггер релиза:**
   - `if: startsWith(github.ref, 'refs/tags/v')` — релиз только для тегов `v*`.
   - Теги `harness-*` остаются CI-сборкой без создания релиза.
4. **Сверка версии:**
   - В `release` job после чтения `[workspace.package].version`:
     ```bash
     tag_version="${GITHUB_REF_NAME#v}"
     if [ "$tag_version" != "$version" ]; then
       echo "Tag '$GITHUB_REF_NAME' does not match Cargo.toml version '$version'" >&2
       exit 1
     fi
     ```
5. **Скачивание артефактов:**
   - `Download Windows artifact`: name `harness-<version>-windows.exe`.
   - Linux/macOS — имена без изменений.
6. **Подготовка ассетов:**
   - `release/harness-<version>-windows.exe`
   - `release/harness-<version>-linux`
   - `release/harness-<version>-macos`
   - Проверить существование всех трёх файлов до вызова action.
   - Посчитать `sha256sum` в `release/*.sha256`.
7. **Создание релиза:**
   - `softprops/action-gh-release@v2` с `generate_release_notes: true`
     и `files: release/*`.
   - `permissions: contents: write` сохраняется.
8. **Вариант реализации:** дорабатываем существующий `ci.yml`
   (MVP, дёшево, всё уже почти есть). Отдельный `release.yml` и `gh` CLI
   не вводятся.

## 7. Сводная таблица изменений по крейтам

| Крейт / файл | CWD first | Output buffer | Deprecated-dir-patterns | Release |
|---|---|---|---|---|
| `conf/src/types.rs` | + | | + | |
| `conf/src/load.rs` | + (CWD first, root validation, ensure_conf_in_cwd, удаление `load_model`, прокидывание CWD в api-key) | | + (glob validation) | |
| `conf/src/fs_scope.rs` | + | | + | |
| `conf/src/glob.rs` (новый) | | | + | |
| `conf/src/str/keys.rs` | + (CWD — отвергаемый корневой ключ) | | + (`deprecated-dir-patterns`) | |
| `conf/src/lib.rs` | + (убрать re-export `load_model`) | | + (`pub mod glob`) | |
| `conf/src/template.rs` | + | | | |
| `conf/settings-template.json5` | + | | + | |
| `conf/Cargo.toml` | | | + (`regex`) | |
| `Cargo.lock` (workspace) | | | + (при необходимости) | |
| `cli-harness/src/cli.rs` | + (`default_conf_path(cwd)`, `--cwd`/`--dump` conflict, help) | | | |
| `cli-harness/src/main.rs` | + (печать cwd/conf, resume из dump CWD) | | | |
| `cli-harness/src/session.rs` | + (`resolve_cwd`, `resolve_abs_path`) | | | |
| `cli-harness/src/server/mod.rs` | + (cwd до загрузки, абсолютный conf path) | | | |
| `cli-harness/src/server/state.rs` | + (`LoadedServerConfig.cwd`) | | | |
| `cli-harness/src/server/config.rs` | + (6 call-site'ов `load_server_config`) | | | |
| `cli-harness/help.txt` | + | | | |
| `agent-loop/src/agent_loop.rs` | + (поле `cwd`, передача в `embed`) | | | |
| `agent-loop/src/dump_settings.rs` | + (`CWD_KEY`, `embed`/`extract` с CWD, удалить `conf_dir_of`) | | | |
| `tools/src/process.rs` | | + | | |
| `tools/src/terminal.rs` | | + | | |
| `tools/src/user.rs` | | + | | |
| `tools/src/lib.rs` | | + (`arg_i64`) | + (`ToolRegistry::build` fallible) | |
| `tools/src/fs.rs` | | | + (`FsModule::build` fallible) | |
| `tools/src/fs/access.rs` | | | + (`Vec<Regex>`, `from_fs` fallible, ancestors) | |
| `tools/help-user.txt` | + (git executable, CWD) | + | + | |
| `harness.json5` (корень проекта) | + | | + | |
| `.github/workflows/ci.yml` | | | | + |
| Unit-тесты `conf` | + | | + | |
| Unit-тесты `tools` | | + | + | |
| Unit-тесты `cli-harness` (parse_args) | + | | | |
| Unit-тесты `agent-loop` (dump CWD) | + | | | |
| Интеграционные/серверные тесты | + (порядок проверок, абсолютный path) | | | |

## 8. Тестовые сценарии, которые необходимо покрыть

### CWD first / dump
1. `harness.json5` без `cwd` → валиден.
2. `harness.json5` с `cwd` → `CONFIG_INVALID` / ошибка запуска с отдельным
   сообщением.
3. Конфиг с относительным путём, выходящим за CWD (`../outside`) → ошибка.
4. Конфиг с абсолютным путём вне CWD → ошибка.
5. `--conf` вне CWD при отсутствующем файле → ошибка запуска, а не `Missing`.
6. `--conf` относительный при `--cwd`, отличном от process cwd, резолвится
   от process cwd и затем проверяется на «внутри CWD».
7. `default_conf_path(Some(cwd))` возвращает `<cwd>/harness.json5`;
   `default_conf_path(None)` — `<process-cwd>/harness.json5`.
8. `--cwd` с `--dump` → ошибка CLI.
9. Новый dump содержит `"settings"` и `"CWD"`; `settings` не содержит `cwd`.
10. Resume читает CWD из `"CWD"` и передаёт его в `load_from_value`.
11. Старый dump с `settings.cwd` или без `"CWD"` → ошибка запуска.
12. `resolve_in_cwd`: `.` / `..` нормализуются; Windows case-insensitivity;
    `C:\work` не считается внутри `C:\work2`.
13. `LoadedServerConfig.path` — абсолютный нормализованный путь.

### Output buffer
1. `output_buff_size = -1` → полный вывод.
2. `output_buff_size = 0` → пустая строка.
3. `output_buff_size = N` → последние `N` Unicode-символов.
4. `output_buff_size < -1` / дробное / bool → ошибка tool call.
5. Стриминг в консоль пользователя остаётся полным.

### Deprecated-dir-patterns
1. `**/log`, `**/done`, `**/note-*` компилируются и матчатся.
2. `log/**`, `**/log/**`, `foo/**/bar`, `**`, `**/log/` — ошибка валидации
   с позицией и справкой.
3. `**/log?` матчит литеральный `log?`, а не `log`.
4. Ancestors: `a/log/x.md` и `a/log` deprecated при паттерне `**/log`;
   `walk_folder` не заходит в запрещённые поддеревья.
5. Windows: `**/Log` запрещает `log`; Unix — нет.
6. `Sandbox::from_fs` с невалидным паттерном возвращает `Err`, а не паникует.

### GitHub release
1. Имя `harness-<ver>-windows.exe` проходит валидацию.
2. Имена `harness-<ver>-windows`, `harness-<ver>-linux.exe`,
   `harness-<ver>-macos.exe` и бессуффиксное — отклоняются.
3. Тег `v*` с версией, не совпадающей с `Cargo.toml`, роняет release job.
4. Тег `harness-*` не создаёт релиз.
