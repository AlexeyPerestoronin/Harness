# Задача на разработку: Output buffer и deprecated-dir-patterns в крейте tools

Доработать крейт `crates/tools` в соответствии с утверждённым планом phase-7 (consulting 3): добавить аргумент `output_buff_size` для `run_shell` и `request_command_shell_execution` с возвратом агенту только последних N Unicode-символов вывода; реализовать matching `deprecated-dir-patterns` в `Sandbox`; сделать сборку fs-модуля и реестра инструментов fallible; актуализировать `help-user.txt`. Изменения ограничиваются крейтом `tools`.

Зачем: дать агенту контроль над объёмом возвращаемого вывода shell-команд и реализовать гибкий запрет директорий/файлов в fs-tools по glob-паттернам вида `**/log`, `**/done`, `**/note-*`.

Ожидаемый результат: крейт `tools` компилируется, unit-тесты `cargo test -p tools` проходят, schema tool-вызовов и `help-user.txt` соответствуют плану phase-7.

## Функциональные требования

### Output buffer

1. В `process.rs` сигнатура меняется на `run_observed_command(cmd, cwd, timeout_secs, output_buff_size)`.
2. Полный вывод по-прежнему стримится в консоль пользователя; ограничение применяется только к строке, возвращаемой агенту.
3. После `decode_command_output` к возвращаемой строке применяется `tail_chars(s, output_buff_size)`.
4. `tail_chars` реализуется без полного `Vec<char>`:
   ```rust
   fn tail_chars(s: &str, n: i64) -> String {
       match n {
           -1 => s.to_string(),
           0 => String::new(),
           _ => s.chars().rev().take(n as usize)
                    .collect::<Vec<_>>().into_iter().rev().collect(),
       }
   }
   ```
5. Семантика `output_buff_size`:
   - `-1` — весь вывод;
   - `0` — пустая строка-результат;
   - `N > 0` — последние `N` Unicode-символов вывода;
   - `< -1` или нецелое — ошибка tool call.
6. В `lib.rs` добавляется `arg_i64(args, key, default) -> Result<i64, String>`:
   - принимает JSON-число, представимое в `i64` (не дробное), или строку, содержащую целое число;
   - отсутствие/null → `default`;
   - отклоняет bool, object, array, дробные числа и значения вне `i64`.
7. В `terminal.rs`:
   - `run_shell` читает `output_buff_size` через `arg_i64(args, "output_buff_size", -1)?`;
   - валидирует диапазон: `-1` или `>= 0`; иначе — ошибка tool call;
   - передаёт значение в `run_observed_command`.
8. Schema `run_shell`:
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
9. В `user.rs`:
   - `request_command_shell_execution` читает `output_buff_size` последним аргументом (default `-1`), валидирует диапазон и передаёт в `run_observed_command`;
   - schema обновляется аналогично `run_shell`.

### Deprecated-dir-patterns matching

10. `Sandbox` хранит `Vec<Regex>` для скомпилированных паттернов.
11. Сигнатура меняется на fallible:
    `Sandbox::from_fs(root, available_dirs, available_files, extensions, deprecated_dirs, deprecated_files, deprecated_dir_patterns) -> Result<Self, String>`:
    - компилирует паттерны через `conf::glob::regex_source`;
    - ошибку компиляции возвращает, а не паникует и не игнорирует.
12. `FsModule::build` становится fallible и возвращает `Result<(Option<Self>, Arc<Mutex<Sandbox>>), String>`.
13. `ToolRegistry::build` становится fallible и возвращает `Result<Self, String>`; ошибка от `FsModule::build` пробрасывается наружу.
14. `is_deprecated_abs` дополнительно проверяет относительный путь (от `Sandbox.root`) и все его ancestors по скомпилированным паттернам:
    - для файла `a/log/x.md` → `a`, `a/log`, `a/log/x.md`;
    - для директории `a/log` → `a`, `a/log`.
15. Паттерн и проверяемый относительный путь нормализуются (слэши `/`, без ведущего слэша); на Windows оба приводятся к нижнему регистру перед матчингом; на Unix матчинг регистрозависимый.
16. `walk_folder` продолжает пропускать deprecated-узлы, в том числе по паттернам, через `is_deprecated` (функция `walk_folder` уже вызывает `is_deprecated`, поэтому отдельная правка её сигнатуры не требуется).
17. Инвариант: в штатном сценарии `conf` уже провалидировал паттерны, поэтому `ToolRegistry::build` не падает; fallible-цепочка защищает от ручного создания `Sandbox` с невалидными значениями.

### `help-user.txt`

18. Описать `output_buff_size` для `run_shell` и `request_command_shell_execution`:
    - последний необязательный аргумент, default `-1`;
    - `-1` — весь вывод; `0` — пустая строка; `N > 0` — последние N Unicode-символов;
    - `< -1` и нецелые значения — ошибка tool call.
19. Описать `tools.fs.deprecated-dir-patterns`:
    - допустимый синтаксис — glob pattern `**/<component>`;
    - внутри компонента единственный wildcard — `*`;
    - `?`, `[`, `]` и regex-метасимволы — литералы;
    - regex не поддерживается;
    - примеры: `**/log`, `**/done`, `**/note-*`;
    - недопустимые: `log/**`, `**/log/**`, `foo/**/bar`, `**`, `**/log/`.
20. Актуализировать описание `tools.git.executable`: это внешняя команда/исполняемый файл; значение передаётся в `Command::new` как есть, не резолвится от CWD и не проверяется на «внутри CWD»; `"git"` ищется в `PATH`.
21. Добавить одну строку о том, что проверка путей — лексическая, symlink/junction не канонизируются (строгая изоляция через symlink не обещается).

## Проверка корректности решения

- `cargo test -p tools` — все тесты крейта проходят.
- `cargo build -p tools` — крейт собирается без ошибок.
- Добавить unit-тесты в `crates/tools`:
  - `tail_chars`: `-1` → полный вывод; `0` → пустая строка; `N` → последние N Unicode-символов (включая многобайтовые символы);
  - `arg_i64`: JSON-число/числовая строка/отсутствие/null/default; отклоняются bool/object/array/дробные/вне `i64`;
  - `run_shell` и `request_command_shell_execution` отклоняют `output_buff_size < -1` и нецелые значения как ошибку tool call;
  - `Sandbox::from_fs` с невалидным паттерном возвращает `Err`, а не паникует;
  - `is_deprecated_abs`: `**/log` матчит `a/log`, `a/log/x.md`; `**/log?` матчит литеральный `log?`, а не `log`;
  - на Windows `**/Log` запрещает `log`; на Unix — нет.

## Scope

Минимально необходимые изменения:

- `crates/tools/src/process.rs`
- `crates/tools/src/terminal.rs`
- `crates/tools/src/user.rs`
- `crates/tools/src/lib.rs`
- `crates/tools/src/fs.rs`
- `crates/tools/src/fs/access.rs`
- `crates/tools/help-user.txt`

Справочные материалы для чтения:

- `WiKi/todo/phase-7/pm-consulting report 3/base.md`
- `WiKi/todo/phase-7/pm-consulting report 3/arch.md`
- `WiKi/todo/phase-7/pm-consulting report 3/tech.md`
- текущие исходники `crates/tools` и `crates/conf`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 2-tools-output-buffer-and-deprecated-patterns → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
