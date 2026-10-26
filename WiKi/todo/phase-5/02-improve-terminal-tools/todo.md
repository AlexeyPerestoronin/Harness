# Задача на разработку: Улучшить `tools.terminal`

1. Полностью удалить инструмент `run_invoke` без обратной совместимости.
2. Полностью удалить инструмент `list_available_shell_commands` без обратной совместимости.
3. Расширить описание инструмента `run_shell`, добавив в него информацию о списке доступных команд и доступных `cwd`.

Зачем: `run_invoke` и `list_available_shell_commands` больше не нужны, а модель должна получать актуальные списки `available-commands` и `cwd-list` прямо в описании `run_shell`, не вызывая отдельный инструмент.

Ожидаемый результат: в наборе инструментов терминала остаётся только `run_shell`; его описание содержит конкретные списки доступных команд и каталогов; код, конфигурация и документация не содержат следов `run_invoke`, `list_available_shell_commands` и `invoke-execution-limit`.

## Функциональные требования

### Удаление `run_invoke`

- Убрать из `crates/tools/src/terminal.rs`:
  - дескриптор `run_invoke`;
  - ветку `"run_invoke"` в `call`;
  - метод `run_invoke`;
  - функцию `invoke_python`.
- Убрать из `crates/tools/src/lib.rs` передачу `term.invoke_execution_limit` в `TerminalModule::new`.
- Убрать из конфигурации:
  - поле `invoke_execution_limit` из `crates/conf/src/types.rs::TerminalSettings`;
  - чтение `invoke-execution-limit` в `crates/conf/src/load.rs::load_terminal`;
  - константу `INVOKE_EXECUTION_LIMIT` из `crates/conf/src/str/keys.rs`.
- Убрать упоминания `run_invoke` и `invoke-execution-limit` из:
  - `crates/conf/settings-template.json5`;
  - `crates/tools/help-user.txt`;
  - `README.md` и `README.ru.md`.

### Удаление `list_available_shell_commands`

- Убрать из `crates/tools/src/terminal.rs`:
  - дескриптор `list_available_shell_commands`;
  - ветку `"list_available_shell_commands"` в `call`;
  - метод `available_commands`, если он больше нигде не используется.
- Убрать упоминания инструмента из `crates/tools/help-user.txt`, `README.md`, `README.ru.md`.

### Расширение описания `run_shell`

- Сделать дескрипторы терминала зависящими от фактических настроек:
  - `TerminalModule::descriptors` должен стать методом `&self` (или иным образом получить доступ к `available_commands` и `cwd_list`);
  - в `crates/tools/src/lib.rs` для `Module::Terminal` вызывать этот метод у конкретного экземпляра, а для остальных модулей оставить существующий способ.
- Описание `run_shell` должно включать:
  - список доступных команд: `available-commands` (например, `["cargo", "git"]`);
  - список разрешённых каталогов: `cwd-list`;
  - правило, что `cwd` должен находиться внутри корневого CWD и внутри одного из `cwd-list` (допускается подкаталог).
- Схема параметров `run_shell` остаётся прежней: `cwd`, `command`, `arguments`; `cwd` и `command` обязательны.

### Валидация `run_shell`

- Сохранить в `TerminalModule` корневую рабочую директорию (`cwd`, передаётся из `settings.cwd`).
- Перед запуском команды проверять, что запрошенный `cwd`:
  - после нормализации `./` и `../` не выходит за пределы корневого CWD;
  - находится внутри одного из `cwd-list` (префиксное совпадение абсолютных путей); записи `cwd-list` при этом также разрешаются относительно корневого CWD, а не относительно process cwd.
- При нарушении возвращать ошибку с указанием запрошенного `cwd`, доступных `cwd-list` и корневого CWD.

### Совместимость конфигурации

- После удаления `invoke-execution-limit` старый ключ в `tools.terminal` больше не используется. Специальной обратной совместимости не добавлять.
- Убедиться, что `tools.terminal` без `invoke-execution-limit` загружается корректно.

## Проверка корректности решения

- `cargo check --workspace` выполняется без ошибок.
- `cargo test --workspace` выполняется без ошибок.
- Поиск по репозиторию не находит в коде и документации: `run_invoke`, `list_available_shell_commands`, `invoke_execution_limit`, `invoke-execution-limit`, `invoke_python`.
- `harness.exe --help-tools` показывает секцию `terminal` с единственным инструментом `run_shell`; в его описании видны конкретные `available-commands` и `cwd-list`.
- Ручная проверка `run_shell`: разрешённый `cwd` под корнем CWD и в `cwd-list` выполняется; `cwd` с `../` за пределы CWD отклоняется; команда не из `available-commands` отклоняется.

## Scope

- `crates/tools/src/terminal.rs`
- `crates/tools/src/lib.rs`
- `crates/tools/help-user.txt`
- `crates/conf/src/types.rs`
- `crates/conf/src/load.rs`
- `crates/conf/src/str/keys.rs`
- `crates/conf/settings-template.json5`
- `README.md`
- `README.ru.md`
