# Результат: единая граница CWD для fs / user / логов

## Было → Стало (почему)

### 1. `crates/tools/src/fs/access.rs`
- **Было:** `Sandbox` не знал про CWD. `abspath` резолвил всё относительно process cwd; `check_access` не проверял выход за CWD; allow/deny-списки сравнивались строковыми префиксами от process cwd.
- **Стало:**
  - В `Sandbox` добавлено поле `root: PathBuf` и конструкторы `new(root)` / `from_fs(root, ...)`; `from_fs` теперь принимает корневой CWD.
  - Введён `resolve_in_root(root, path)`: относительный путь джойнится к `root`, абсолютный оставляется как есть, затем путь лексически нормализуется; итог обязан начинаться с `root` по компонентам (на Windows — case-insensitive).
  - `check_access`, `is_deprecated`, `grant_path_access`, `is_allowed_abs` используют root-aware резолвинг; путь вне CWD отклоняется до проверки allow/deny-списков.
  - Allow- и deny-записи конфигурации по-прежнему хранятся в исходном виде, но каждая запись резолвится относительно `root` в момент проверки; запись, выходящая за CWD, не даёт доступа и не срабатывает как deprecated.
  - `grant_path_access` классифицирует `is_dir` по root-resolved пути, а не по process cwd.
- **Почему:** CWD стал единой границей файлового sandbox; `../../../outside` больше не может быть разрешён, даже если путь попал бы в allow-list.

### 2. `crates/tools/src/fs.rs`
- **Было:** `FsModule::build` создавал `Sandbox` без CWD; все операции `fs::read/write/rename/...` выполнялись по исходной строке пути, то есть фактически относительно process cwd.
- **Стало:**
  - `FsModule::build(settings, cwd)` принимает корневой CWD и передаёт его в `Sandbox`.
  - `guard` возвращает `PathBuf` — абсолютный путь внутри CWD; все операции read/write/create/remove/move/list/create_folder/remove_folder/move_folder выполняются уже по этому абсолютному пути.
  - Для `list_folder` путь к обходу также резолвится через sandbox.
- **Почему:** проверка доступа и фактическая файловая операция теперь работают в одних координатах — относительно CWD, а не process cwd.

### 3. `crates/tools/src/fs/search.rs`
- **Было:** `find_*` проверяли root через sandbox, но читали `Path::new(root)` от process cwd.
- **Стало:** `iter_files_for_search` резолвит root относительно CWD и обходит уже абсолютный root; файлы возвращаются как абсолютные пути внутри CWD. Блокировка sandbox не удерживается во время рекурсивного обхода.
- **Почему:** поиск больше не может выйти за CWD и не провоцирует deadlock на Mutex.

### 4. `crates/tools/src/lib.rs`
- **Было:** `ToolRegistry::build` вызывал `FsModule::build(settings.fs.as_ref())`.
- **Стало:** `FsModule::build(settings.fs.as_ref(), &ctx.cwd)`.
- **Почему:** корневой CWD из `ToolsContext` доходит до sandbox.

### 5. `crates/tools/src/user.rs`
- **Было:** `request_command_shell_execution` запускал команду в произвольном `cwd`; `request_read_access_for` / `request_write_access_for` могли выдать доступ за CWD.
- **Стало:**
  - Перед запросом разрешения `request_*_access_for` проверяют `resolve_in_root(path)`.
  - `request_command_shell_execution` резолвит `cwd` через sandbox и запускает команду в полученном абсолютном пути внутри CWD; вне CWD запрос отклоняется до prompt.
  - `grant_path_access` сам тоже root-aware, поэтому доступ за CWD не выдаётся.
- **Почему:** user-инструменты больше не позволяют модели расширить границу через runtime-гранты или shell-команду.

### 6. `crates/harness/src/agent_loop.rs`
- **Было:** `session_dir` делал `settings.cwd.join(log_folder)`: абсолютный `log-folder` или `..` могли создать каталог логов вне CWD.
- **Стало:** `log_root = tools::fs::resolve_in_root(&settings.cwd, &settings.log_folder)`; при выходе за CWD запуск завершается ошибкой `invalid log-folder`.
- **Почему:** каталог сессии гарантированно находится внутри CWD.

### 7. `crates/conf/src/fs_scope.rs` + `crates/conf/src/load.rs`
- **Было:** конфликты `tools.fs` считались от process cwd (`lexical_abspath` без CWD).
- **Стало:** `check_conflicts(cwd, ...)` принимает CWD; все `available-*` / `deprecated-*` резолвятся относительно переданного CWD. `load_tools` / `load_fs` прокидывают `cwd` из `load_from_value`; CWD нормализуется перед сравнением.
- **Почему:** конфликты allow-list/denylist считаются в тех же координатах, что и sandbox, даже если `settings.cwd` отличается от process cwd.

## Проверка
- `cargo check --workspace` — успешно.
- `cargo test --workspace` — успешно (все тесты, включая новые 5 root-aware тестов `Sandbox`).
- Добавлены юнит-тесты в `crates/tools/src/fs/access.rs`:
  - относительный путь внутри CWD;
  - `sub/../file` внутри CWD;
  - `../outside` отклоняется;
  - абсолютный путь внутри CWD допускается, вне CWD отклоняется;
  - `available-dirs` / `deprecated-dirs` трактуются относительно CWD.
