# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: English tool descriptions
Переписать на английский язык (`EN`) все `description` у tool descriptors, которые агент видит в vendor API.

Зачем: модели лучше следуют английским описаниям инструментов; сейчас все описания в `ToolDescriptor.description` на русском.

Ожидаемый результат: каждый зарегистрированный инструмент отдаёт краткое английское описание той же семантики; имена инструментов, JSON Schema `parameters`, поведение `call` и пользовательские stdin-промпты (`Запрос read-доступа…` и т.п.) не меняются.

## Функциональные требования
1. Заменить **только** поле `description` у всех `ToolDescriptor`, которые сейчас задаются в:
   - `crates/tools/src/fs.rs`
   - `crates/tools/src/search.rs`
   - `crates/tools/src/git.rs`
   - `crates/tools/src/terminal.rs`
   - `crates/tools/src/meta.rs`
   - `crates/tools/src/user.rs`
2. Смысл описания должен совпадать с текущим русским текстом (не «улучшать» и не исправлять расхождения описания с кодом). Язык — английский. Стиль — короткий императив / noun phrase, как сейчас.
3. Обязательный набор (имя → целевое EN-описание):
   - `read_file` → `Read the contents of a file.`
   - `write_file` → `Write a file.`
   - `create_file` → `Create a file.`
   - `remove_file` → `Delete a file.`
   - `move_file` → `Move a file.`
   - `list_folder` → `Return the contents of a directory to the given depth (depth >= 0, where 0 is root contents only).`
   - `create_folder` → `Create a folder.`
   - `remove_folder` → `Delete a folder.`
   - `move_folder` → `Move a folder.`
   - `find_str` → `Search for a string in files starting from the root directory.`
   - `find_str_by_regex` → `Search for lines matching a regular expression starting from the root directory.`
   - `is_file_under_git` → `Check whether a file is tracked by git.`
   - `apply_diff_patch` → `Apply a diff patch to files (git-style, because it uses git apply under the hood).`
   - `get_file_diff` → `Get the diff for the target file.`
   - `discard_file_changes` → `Discard file changes after HEAD.`
   - `list_available_file_extension` → `List of available file extensions.`
   - `list_available_r_dir` → `List of directories available for reading.`
   - `list_available_w_dir` → `List of directories available for writing.`
   - `list_available_r_file` → `List of files available for reading.`
   - `list_available_w_file` → `List of files available for writing.`
   - `list_available_shell_commands` → `List of available shell commands.`
   - `run_shell` → `Run a command.`
   - `run_invoke` → `Run an invoke command.`
   - `ask_user` → `Ask the user a question and get the answer.`
   - `request_read_access_for` → `Request read access to a folder/file from the user.`
   - `request_write_access_for` → `Request write access to a folder/file from the user.`
   - `request_command_shell_execution` → `Request permission from the user to execute a shell command.`
4. Не переводить и не трогать: ключи JSON Schema, имена инструментов, тексты ошибок `call`, stdin-диалоги в `user.rs`, сообщения CLI, комментарии в коде, json5-шаблоны.
5. Существующие unit-тесты, которые не проверяют текст `description`, должны пройти без изменений. Если теста на тексты нет — не добавлять, кроме случая, когда без него нельзя убедиться, что ни одно описание не осталось на русском.

## Scope
- `crates/tools/src/fs.rs`
- `crates/tools/src/search.rs`
- `crates/tools/src/git.rs`
- `crates/tools/src/terminal.rs`
- `crates/tools/src/meta.rs`
- `crates/tools/src/user.rs`

## Проверка корректности решения
1. `cargo test -p tools`
2. Убедиться, что в перечисленных файлах в аргументах `desc(...)` / `ToolDescriptor { description: ... }` нет кириллицы.
3. Ручной прогон агента не требуется.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
