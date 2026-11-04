# Задача на разработку: CWD first в cli-harness и agent-loop

Реализовать CWD first в `crates/cli-harness` и `crates/agent-loop` в соответствии с утверждённым планом phase-7 (consulting 3): CWD определяется только запуском (`--cwd` или process cwd), выводится в консоль до начала цикла, `--conf`/`--dump` резолвятся от process cwd, dump получает верхнеуровневый `"CWD"`, resume читает CWD из dump. Обновить `help.txt` и корневой `harness.json5`.

Зачем: чтобы `harness.exe` можно было запускать из любого места системы как системную утилиту, а рабочая директория строго задавалась местом запуска или явным `--cwd`, и чтобы возобновление сессии из dump восстанавливало ту же рабочую директорию.

Ожидаемый результат: крейты `cli-harness` и `agent-loop` компилируются, их unit-тесты проходят, консольный и server-режимы работают по модели CWD first, dump пишет и читает `"CWD"`.

## Функциональные требования

### `cli-harness/src/cli.rs`

1. `--cwd` задаёт сам CWD, а не override json5-поля.
2. `default_conf_path(cwd: Option<&Path>)`:
   - нет `--cwd` → `<process-cwd>/harness.json5`;
   - есть `--cwd` → `<CWD>/harness.json5`.
3. Один и тот же `default_conf_path(cwd)` используется и для `Start`, и для `Serve`; `default_conf_path_for_serve` удаляется.
4. `--dump` не комбинируется с `--cwd` (CWD берётся из dump); такая комбинация — ошибка CLI.
5. `--conf` с `--dump` по-прежнему запрещён.
6. Обновить `help.txt`:
   - `--cwd` — «workspace root»;
   - `--conf` — default `<cwd>/harness.json5` в обоих режимах;
   - `--dump` — resume; CWD берётся из `"CWD"` в dump; `--cwd` не требуется;
   - явно указать, что относительные `--conf`/`--dump` резолвятся от process cwd, а `--conf` обязан оказаться внутри CWD.
7. Unit-тесты `parse_args` покрывают:
   - `default_conf_path(Some(cwd))` = `<cwd>/harness.json5`, `default_conf_path(None)` = `<process-cwd>/harness.json5`;
   - `--cwd` с `--dump` → ошибка CLI.

### `cli-harness/src/session.rs`

8. `resolve_cwd_override(raw, conf_dir)` заменяется на `resolve_cwd(raw)`:
   - абсолютный → `normalize_abs`;
   - относительный → `current_dir()?.join(raw)` → `normalize_abs`.
9. Добавляется `resolve_abs_path(raw)`: относительный `--conf`/`--dump` резолвится от **process cwd** и нормализуется; абсолютный — только нормализуется. Используется и для фактического открытия файла, и для вывода в консоль.
10. `resolve_api_key_override(cwd, raw)` остаётся: относительный путь резолвится от CWD, абсолютный используется как есть.

### `cli-harness/src/main.rs`

11. В `start_new_session`:
    ```rust
    let cwd = session::resolve_cwd(cwd_override.as_deref())?;
    let conf_path = session::resolve_abs_path(&conf_path)?;
    println!("cwd: {}", cwd.display());
    println!("conf: {}", conf_path.display());
    let state = conf::load_server_config(&conf_path, &cwd)?;
    ```
    Пост-загрузочный блок `if let Some(raw) = cwd { settings.cwd = ... }` удаляется.
12. Для `Start` без `--dump` используется `conf::load_server_config(&conf_path, &cwd)`.
13. Для `--dump`:
    ```rust
    let dump_path = session::resolve_abs_path(&dump_path)?;
    let (settings_value, cwd) = agent_loop::dump_settings::extract(&dump_path)?;
    println!("cwd: {}", cwd.display());
    println!("settings: from dump");
    let settings = conf::load_from_value(settings_value, &cwd, chooser)?;
    ```
14. Сигнатура `start_resume_session` меняется на `(dump_path, api_key)` — параметр `cwd` из неё убирается. Вызов `agent_loop::dump_settings::conf_dir_of` удаляется.
15. Вывод `cwd` / `conf` / `settings` выполняется до интерактивного выбора агента и до первой итерации цикла.
16. Если CWD из dump не существует или недоступен, запуск завершается ошибкой до начала цикла.

### `cli-harness/src/server/mod.rs`, `server/state.rs`, `server/config.rs`

17. В `serve` сначала вычислить `cwd = session::resolve_cwd(options.cwd.as_deref())?`, затем `conf_path = session::resolve_abs_path(&conf_path)?`, затем `conf::load_server_config(&conf_path, &cwd)`.
18. Удалить блок, который после загрузки переопределял `settings.cwd` через `resolve_cwd_override`.
19. Передавать `cwd` и абсолютный `conf_path` в `LoadedServerConfig`.
20. Startup info печатает `cwd` из `LoadedServerConfig.cwd`, даже если конфиг `Missing` / `Invalid`.
21. В `LoadedServerConfig` добавляется `pub cwd: PathBuf` — эффективный CWD сервера; гарантия: `path` и `cwd` — абсолютные нормализованные пути.
22. Все вызовы `conf::load_server_config` в `server/config.rs` получают вторым аргументом CWD (шесть call-site'ов):
    - `get_config`;
    - `config_default`;
    - `validate_config_text(text, conf_dir, target_path, cwd)`;
    - `read_existing_parsed(path, cwd)`;
    - `reload_server_config(state, path, cwd)`;
    - `refresh_server_config(state, path, cwd)`.
23. `validate_config_text` прокидывает CWD сервера в загрузку временного файла, а не полагается на process cwd.
24. `PUT /api/v1/config` берёт CWD из `state.config.cwd` и передаёт его в валидацию и перезагрузку.
25. `config_default` в ветке `Missing` после записи шаблона вызывает `refresh_server_config` и возвращает фактическое состояние валидации (`valid`), а не константу `true`.

### `agent-loop/src/agent_loop.rs` и `dump_settings.rs`

26. `AgentLoop` получает поле `cwd: PathBuf`, сохраняемое в `new`.
27. При каждой записи dump вызывается `dump_settings::embed(dump_file, &self.dump_settings, &self.cwd)`.
28. `AgentLoop::new` пробрасывает ошибку от `ToolRegistry::build` (fallible-сигнатура) через `?`.
29. В `dump_settings.rs`:
    - добавляется `pub const CWD_KEY: &str = "CWD";`;
    - `embed(path, settings, cwd)` записывает верхнеуровневые `"settings"` и `"CWD"` (значение `cwd` — абсолютная нормализованная строка), сохраняя vendor-поля dump;
    - `extract(path) -> Result<(Value, PathBuf), String>` возвращает `(settings, cwd)`; при отсутствии `"CWD"` или неабсолютном значении — ошибка;
    - удаляются `conf_dir_of` и все его использования;
    - `settings.source` больше не содержит `cwd`.

### Корневой `harness.json5`

30. Убрать ключ `cwd` и его комментарий.
31. В `tools.fs` добавить ключ `deprecated-dir-patterns` (массив строк) с закомментированными примерами `**/log`, `**/done`, `**/note-*` (или пустой массив), в соответствии с планом.
32. Убедиться, что после правок конфиг остаётся валидным при CWD = корень проекта.

## Проверка корректности решения

- `cargo test -p cli-harness` и `cargo test -p agent-loop` проходят.
- `cargo build -p cli-harness` собирается без ошибок.
- Добавить/обновить unit-тесты:
  - `parse_args`: `--cwd` с `--dump` → ошибка; `default_conf_path` для `Some(cwd)` и `None`.
  - `session`: `resolve_cwd` для абсолютного и относительного пути; `resolve_abs_path` резолвится от process cwd.
  - `dump_settings`: `embed` пишет `"settings"` и `"CWD"`, а `settings` не содержит `cwd`; `extract` возвращает `(settings, cwd)`; dump без `"CWD"` или с неабсолютным `"CWD"` — ошибка.
- Ручные сценарии:
  - `cargo run -p cli-harness -- --prompt "..."` из корня проекта выводит `cwd:` и `conf:` до выбора агента;
  - `cargo run -p cli-harness -- --dump <dump.json5>` возобновляется с CWD из `"CWD"` dump;
  - `--conf` вне CWD при отсутствующем файле → ошибка запуска, а не «configuration file does not exist»;
  - server-режим: `GET /api/v1/config` возвращает абсолютный `path`, а `valid` отражает CWD first валидацию; `PUT /api/v1/config` с путём вне CWD → `CONFIG_INVALID`; `PUT /api/v1/config` с `tools.git.executable: "git"` → валиден.

## Scope

Минимально необходимые изменения:

- `crates/cli-harness/src/cli.rs`
- `crates/cli-harness/src/session.rs`
- `crates/cli-harness/src/main.rs`
- `crates/cli-harness/src/server/mod.rs`
- `crates/cli-harness/src/server/state.rs`
- `crates/cli-harness/src/server/config.rs`
- `crates/cli-harness/help.txt`
- `crates/agent-loop/src/agent_loop.rs`
- `crates/agent-loop/src/dump_settings.rs`
- `harness.json5` (корень проекта)

Справочные материалы для чтения:

- `WiKi/todo/phase-7/pm-consulting report 3/base.md`
- `WiKi/todo/phase-7/pm-consulting report 3/arch.md`
- `WiKi/todo/phase-7/pm-consulting report 3/api.md`
- `WiKi/todo/phase-7/pm-consulting report 3/tech.md`
- текущие исходники `crates/cli-harness`, `crates/agent-loop` и `crates/conf`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 3-cli-harness-cwd-first-and-dump-resume → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
