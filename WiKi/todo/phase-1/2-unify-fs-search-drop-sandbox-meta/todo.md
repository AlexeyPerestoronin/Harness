# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: Unify fs+search, drop sandbox/meta
Объединить файловые и поисковые инструменты в один модуль `fs`, добавить поиск файлов по имени, удалить отдельные модули `sandbox` и `meta`. Allow-list (`available-dirs`, `available-file-extensions`, `extra-scope`) должен персонально принадлежать объединённому `fs`.

Зачем: search и fs — одна файловая поверхность; отдельный `sandbox.rs` и introspect-модуль `meta` дублируют владение теми же списками доступа. Имена файлов сейчас искать нельзя (есть только поиск по содержимому).

Ожидаемый результат: conf-ключ `tools.search` и `tools.meta` больше не существуют; `sandbox.rs` и `meta.rs` удалены; `tools.fs` владеет allow-list и отдаёт бывшие search/meta fs-инструменты плюс два новых; `git`/`user`/`terminal` продолжают работать через API, которым владеет `fs` (не через отдельный crate-модуль `sandbox`).

## Функциональные требования
### 1. Один tool-модуль `fs`
1. Логика `crates/tools/src/fs.rs` и `crates/tools/src/search.rs` становится одним tool-модулем с conf-ключом `fs`. Отдельного модуля `search` в registry и в conf быть не должно.
2. Файл `crates/tools/src/search.rs` удалить. Внутреннее разбиение внутри `fs` (подмодули `fs/...`) допустимо только если это не отдельный tool-модуль и не conf-ключ.
3. При `tools.fs = Some(...)` registry регистрирует **все** fs-инструменты, включая бывшие search. Если ключа `fs` нет — нет ни файловых, ни поисковых инструментов.
4. Поведение существующих fs/search инструментов (`read_file` … `move_folder`, `find_str`, `find_str_by_regex`, включая progress в stdout у content-search) сохранить.

### 2. Новые инструменты
Добавить в `fs` два инструмента. Описания — на английском (как остальные после предыдущей задачи).

**`find_file_by_str`**
- Параметры: `root` (string, required), `string` (string, required).
- Описание: `Find files whose names contain the given string, starting from the root directory.`
- Рекурсивно обходит `root` (файл или каталог, как `find_str`).
- Сопоставляет **имя файла** (basename), не путь целиком и не содержимое. Каталоги не возвращать.
- Подстрока: обычный `contains`, без regex, без case-folding.
- В выдачу попадают только пути, прошедшие fs allow-list (read + extension), по тем же правилам, что `find_str`.
- Результат: абсолютные/как-есть пути, по одному на строку, без дублей. Нет совпадений → пустая строка.
- Ошибки в том же стиле: `cannot find_file_by_str → ...`.

**`find_files_by_regex`**
- Параметры: `root` (string, required), `regex` (string, required).
- Описание: `Find files whose names match the given regular expression, starting from the root directory.`
- Как `find_file_by_str`, но basename матчится через `regex` crate (`Regex::new` + `is_match`).
- Невалидный regex → ошибка `cannot find_files_by_regex → ...` (аналог `find_str_by_regex`).
- Имя инструмента — точно `find_files_by_regex` (множественное `files`).

### 3. Удалить `sandbox.rs` и `meta.rs`
1. Удалить `crates/tools/src/sandbox.rs`. Типы/проверки доступа (`check_access`, `check_extensions`, `grant_path_access`, `abspath`, разбор `r:`/`w:`) переносятся **внутрь fs-модуля**. Это больше не отдельный tool-модуль и не `pub mod sandbox`.
2. `FsModule` персонально владеет:
   - `available-dirs`
   - `available-file-extensions`
   - `extra-scope`
   Семантика прежняя: prefix-match по `abspath` для dirs; exact `abspath` для extra-scope; `w:` ⇒ `r:`; пустой список расширений = любые; runtime-grant через `request_*_access_for` по-прежнему мутирует эти списки.
3. `git` и `user` больше не зависят от `crate::sandbox::Sandbox`. Они получают доступ к **тому же** состоянию, которым владеет `fs` (например `Arc<Mutex<...>>` типа, объявленного в `fs`). Не копировать allow-list в git/user. Не вводить новый conf-ключ для sandbox.
4. `abspath` нужен `terminal` для `cwd-list`. После удаления `sandbox.rs` реэкспортировать `abspath` из `fs` (или из корня `tools`), без файла `sandbox.rs`.
5. Удалить `crates/tools/src/meta.rs` и conf-ключ `tools.meta`.
6. Бывшие meta-инструменты распределить по владельцам данных:
   - `list_available_file_extension`, `list_available_r_dir`, `list_available_w_dir`, `list_available_r_file`, `list_available_w_file` → модуль `fs` (живые списки, включая runtime grants). Формат ответа тот же (`available: ...` / `available: all`).
   - `list_available_shell_commands` → модуль `terminal` (список `available-commands`). Если `terminal` выключен, инструмента нет (не держать пустой meta-модуль).
7. Имена этих инструментов не менять.

### 4. Conf / template / registry
1. В `crates/conf/src/str/keys.rs` удалить `TOOL_SEARCH` и `TOOL_META` (и связанные пустые типы `SearchSettings` / `MetaSettings`, если больше не используются).
2. `load_tools`: ключи `search` и `meta` — ошибка `unknown tool module`, как любой неизвестный ключ.
3. `--conf-new` шаблон (`crates/conf/src/template.rs`): нет блоков `search` и `meta`; комментарий у `fs` перечисляет файловые, поисковые и list_available_* (кроме shell); комментарий у `terminal` упоминает `list_available_shell_commands`.
4. `ToolRegistry::build` не создаёт Search/Meta модули; sandbox не собирается снаружи fs.
5. Обновить описания/комментарии: `crates/tools/src/lib.rs`, `crates/tools/Cargo.toml`, тесты `crates/tools` и `crates/conf`, которые включают `settings.search` / `settings.meta` / `pub mod search` / `pub mod meta` / `pub mod sandbox`.
6. Корневой `harness.json5` в репозитории **не** обязан править эта задача (он может быть локальным). Шаблон `--conf-new` — обязан.

### 5. Тесты
Сохранить и перенести тесты search/sandbox/meta. Добавить тесты на:
- `find_file_by_str` / `find_files_by_regex` (совпадение по имени, не по содержимому; regex error; access denied);
- registry: при одном только `fs` видны `find_str`, `find_file_by_str`, `list_available_r_dir`; нет отдельного search/meta;
- conf: `tools.search` / `tools.meta` → ошибка загрузки;
- grant write через бывший sandbox API, живущий в fs, по-прежнему виден в `list_available_w_*`.

## Scope
- `crates/tools/src/lib.rs`
- `crates/tools/src/fs.rs` (и опционально `crates/tools/src/fs/` при внутреннем сплите)
- `crates/tools/src/search.rs` (удалить)
- `crates/tools/src/sandbox.rs` (удалить)
- `crates/tools/src/meta.rs` (удалить)
- `crates/tools/src/git.rs`
- `crates/tools/src/user.rs`
- `crates/tools/src/terminal.rs`
- `crates/tools/Cargo.toml`
- `crates/conf/src/str/keys.rs`
- `crates/conf/src/types.rs`
- `crates/conf/src/load.rs`
- `crates/conf/src/template.rs`
- `crates/conf/src/lib.rs` (тесты)

Не трогать vendor-агентов и CLI, если компилятор не потребует (не должен).

## Проверка корректности решения
1. `cargo test --workspace`
2. Убедиться, что файлов `crates/tools/src/search.rs`, `sandbox.rs`, `meta.rs` нет.
3. `harness.exe --conf-new` (во временный путь) — в шаблоне нет ключей `search`/`meta`, есть расширенный `fs`.
4. Загрузка json5 с `"search": {}` или `"meta": {}` должна падать с `unknown tool module`.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
