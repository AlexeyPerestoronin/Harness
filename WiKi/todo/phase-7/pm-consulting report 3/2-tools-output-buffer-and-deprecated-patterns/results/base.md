# Phase-7, задача 2: Output buffer и deprecated-dir-patterns в крейте tools — базовый отчёт

## Было

- `process::run_observed_command(cmd, cwd, timeout_secs)` всегда возвращал агенту полный декодированный вывод shell-команды.
- В `terminal::run_shell` и `user::request_command_shell_execution` не было аргумента `output_buff_size`; schema tool-вызовов не описывала его.
- В `lib.rs` не было универсального парсера целочисленного аргумента tool call (`arg_i64`).
- `Sandbox` хранил только точные `deprecated_dirs` / `deprecated_files` и не умел работать с `tools.fs.deprecated-dir-patterns`.
- `Sandbox::from_fs`, `FsModule::build` и `ToolRegistry::build` были infallible: `Sandbox::from_fs` не компилировал паттерны, а невалидные паттерны могли попасть в рантайм только при ручном создании `Sandbox`.
- `help-user.txt` не описывал `output_buff_size`, `deprecated-dir-patterns`, уточнённую семантику `tools.git.executable` и лексичность проверок путей.

## Стало

### Output buffer

- `process::run_observed_command(cmd, cwd, timeout_secs, output_buff_size)`:
  - полный вывод по-прежнему стримится в консоль пользователя;
  - после `decode_command_output` к строке, возвращаемой агенту, применяется `tail_chars`.
- `tail_chars(s, n)` реализован без полного `Vec<char>`:
  - `-1` — весь вывод;
  - `0` — пустая строка;
  - `N > 0` — последние `N` Unicode-символов.
- В `lib.rs` добавлен `arg_i64(args, key, default) -> Result<i64, String>`:
  - принимает JSON-число, представимое в `i64` (не дробное), или строку с целым числом;
  - отсутствие / `null` → `default`;
  - отклоняет bool, object, array, дробные числа и значения вне `i64`.
- В `lib.rs` добавлен `validate_output_buff_size(value)`: допустимы `-1` или `>= 0`; иначе — ошибка tool call.
- `terminal::run_shell`:
  - читает `output_buff_size` через `arg_i64(args, "output_buff_size", -1)?`;
  - валидирует диапазон через `validate_output_buff_size`;
  - передаёт значение в `run_observed_command`.
- `user::request_command_shell_execution`:
  - читает `output_buff_size` последним необязательным аргументом (default `-1`);
  - валидирует диапазон и передаёт значение в `run_observed_command`.
- Schema `run_shell` и `request_command_shell_execution` дополнена свойством:
  ```json
  "output_buff_size": {
    "type": "integer",
    "description": "N символов с конца; -1 = весь вывод"
  }
  ```

### Deprecated-dir-patterns

- `Sandbox` хранит `Vec<Regex>` для скомпилированных паттернов.
- `Sandbox::from_fs(root, available_dirs, available_files, extensions, deprecated_dirs, deprecated_files, deprecated_dir_patterns) -> Result<Self, String>`:
  - компилирует каждый паттерн через `conf::glob::regex_source` + `Regex::new`;
  - ошибку компиляции возвращает, а не паникует и не игнорирует.
- `FsModule::build` стал fallible и возвращает `Result<(Option<Self>, Arc<Mutex<Sandbox>>), String>`.
- `ToolRegistry::build` стал fallible и возвращает `Result<Self, String>`; ошибка от `FsModule::build` пробрасывается наружу.
- `is_deprecated_abs` дополнительно проверяет относительный путь от `Sandbox.root` и все его ancestors по скомпилированным паттернам:
  - файл `a/log/x.md` → `a`, `a/log`, `a/log/x.md`;
  - директория `a/log` → `a`, `a/log`.
- Паттерн и относительный путь нормализуются (слэши `/`, без ведущего слэша); на Windows оба приводятся к нижнему регистру; на Unix матчинг регистрозависимый.
- `walk_folder` продолжает пропускать deprecated-узлы через `is_deprecated`, поэтому запрещённые поддеревья не обходятся.
- Сообщение об ошибке `check_access` дополнено упоминанием `deprecated-dir-patterns`.

### help-user.txt

- Описан `output_buff_size` для `run_shell` и `request_command_shell_execution`: последний необязательный аргумент, default `-1`; `-1` — весь вывод, `0` — пустая строка, `N > 0` — последние N Unicode-символов; `< -1` и нецелые — ошибка tool call.
- Описан `tools.fs.deprecated-dir-patterns`: синтаксис `**/<component>`, единственный wildcard `*`, `?` / `[` / `]` / regex-метасимволы — литералы, regex не поддерживается; примеры допустимых и недопустимых паттернов.
- Актуализирован `tools.git.executable`: внешняя команда/исполняемый файл; значение передаётся в `Command::new` как есть, не резолвится от CWD и не проверяется на «внутри CWD»; `"git"` ищется в `PATH`.
- Добавлено замечание о том, что проверка путей лексическая, symlink/junction не канонизируются, строгая изоляция через symlink не обещается.

## Почему

- Агент получает контроль над объёмом возвращаемого вывода shell-команд и не тратит контекст на гигантские логи; пользователь по-прежнему видит полный вывод.
- Обрезка по Unicode-символам не рвёт UTF-8 и интуитивно понятна модели.
- Единый `arg_i64` убирает дублирование парсинга числовых tool-аргументов и задаёт строгие правила отклонения невалидных значений.
- `deprecated-dir-patterns` даёт гибкий запрет директорий/файлов по glob без поддержки произвольных regex; fallible-цепочка защищает рантайм от невалидных паттернов при ручном создании `Sandbox`.

## Проверка

- `cargo test -p tools` — 16 тестов проходят.
- `cargo build -p tools` — собирается без ошибок и предупреждений.
- Добавлены unit-тесты:
  - `tail_chars`: `-1` → полный вывод; `0` → пустая строка; `N` → последние N Unicode-символов (включая многобайтовые);
  - `arg_i64`: JSON-число / числовая строка / отсутствие / null / default; отклоняются bool / object / array / дробные / вне `i64`;
  - `run_shell` и `request_command_shell_execution` отклоняют `output_buff_size < -1` и нецелые значения как ошибку tool call;
  - `Sandbox::from_fs` с невалидным паттерном возвращает `Err`, а не паникует;
  - `is_deprecated_abs`: `**/log` матчит `a/log`, `a/log/x.md`; `**/log?` матчит литеральный `log?`, а не `log`;
  - на Windows `**/Log` запрещает `log`; на Unix — нет.

## Исправление ошибок компиляции workspace

### Было

- В `crates/agent-loop/src/agent_loop.rs` вызов `ToolRegistry::build(...)` оборачивался в `Arc::new(...)` без учёта нового fallible-контракта, из-за чего `registry` имел тип `Arc<Result<ToolRegistry, String>>` и `cargo build --workspace` падал с ошибками E0308.

### Стало

- Результат `ToolRegistry::build(...)` пробрасывается из `AgentLoop::new` через `?`:
  ```rust
  let registry = Arc::new(ToolRegistry::build(
      &tools,
      ToolsContext { ... },
  )?);
  ```

### Почему

- `ToolRegistry::build` стал fallible по плану phase-7; вызывающий код обязан обработать `Result`, чтобы ошибка конструирования реестра (например, невалидный `deprecated-dir-patterns`) корректно возвращалась наружу, а не порождала ошибку типов.

### Проверка

- `cargo build --workspace` — успешно, без ошибок и предупреждений.
- `cargo test --workspace` — все тесты проходят.
