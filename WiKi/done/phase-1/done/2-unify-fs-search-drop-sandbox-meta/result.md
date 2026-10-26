# Результат: Unify fs+search, drop sandbox/meta

- отдельные `tools.search` / `tools.meta` / `crates/tools/src/search.rs` / `meta.rs` / `sandbox.rs` → один tool-модуль `fs` с внутренними `fs/access.rs` и `fs/search.rs` (search и fs — одна файловая поверхность; sandbox/meta дублировали allow-list)
- `tools.fs` регистрировал только file/folder ops → регистрирует file/folder + `find_str` / `find_str_by_regex` + `find_file_by_str` / `find_files_by_regex` + `list_available_{file_extension,r_dir,w_dir,r_file,w_file}` (нет ключа `fs` — нет ни файловых, ни поисковых инструментов)
- поиск только по содержимому → плюс поиск по basename: `contains` (`find_file_by_str`) и `regex` (`find_files_by_regex`); каталоги не возвращаются; allow-list как у `find_str`
- `list_available_shell_commands` жил в `meta` → модуль `terminal` (если terminal выключен, инструмента нет)
- git/user/registry собирали `crate::sandbox::Sandbox` снаружи → тип и сборка allow-list принадлежат `fs` (`FsModule::build` + `Arc<Mutex<Sandbox>>`); `abspath` реэкспортируется из `fs`
- conf-ключи `TOOL_SEARCH` / `TOOL_META`, типы `SearchSettings` / `MetaSettings` → удалены; `"search": {}` / `"meta": {}` → `unknown tool module`
- `--conf-new`: блоки `search`/`meta` → нет; комментарий `fs` перечисляет file/search/list_available_* (кроме shell); комментарий `terminal` упоминает `list_available_shell_commands`
- проверка: `cargo test --workspace` зелёный (conf 13, tools 30, harness 12)
