# English tool descriptions

Все `ToolDescriptor.description` в `crates/tools/src/{fs,search,git,terminal,meta,user}.rs` переведены на EN по обязательному списку. Имена, JSON Schema, `call`, stdin-промпты не трогались.

`cargo test -p tools`: 26 passed.

## fs.rs
- `read_file`: Прочитать содержимое файла. → Read the contents of a file. (vendor API EN)
- `write_file`: Записать файл. → Write a file. (vendor API EN)
- `create_file`: Создать файл. → Create a file. (vendor API EN)
- `remove_file`: Удалить файл. → Delete a file. (vendor API EN)
- `move_file`: Переместить файл. → Move a file. (vendor API EN)
- `list_folder`: Возвращает содержимое директории на заданную глубину (depth >= 0, где 0 - только корневое содержимое). → Return the contents of a directory to the given depth (depth >= 0, where 0 is root contents only). (vendor API EN)
- `create_folder`: Создать папку. → Create a folder. (vendor API EN)
- `remove_folder`: Удалить папку. → Delete a folder. (vendor API EN)
- `move_folder`: Переместить папку. → Move a folder. (vendor API EN)

## search.rs
- `find_str`: Поиск строки в файлах начиная с корневого каталога. → Search for a string in files starting from the root directory. (vendor API EN)
- `find_str_by_regex`: Поиск строк по регулярному выражению начиная с корневого каталога. → Search for lines matching a regular expression starting from the root directory. (vendor API EN)

## git.rs
- `is_file_under_git`: Проверить, находится ли файл под контролем git. → Check whether a file is tracked by git. (vendor API EN)
- `apply_diff_patch`: Применить diff-патч к файлам (в стиле git т.к. под капотом git apply). → Apply a diff patch to files (git-style, because it uses git apply under the hood). (vendor API EN)
- `get_file_diff`: Получить diff для целевого файла. → Get the diff for the target file. (vendor API EN)
- `discard_file_changes`: Отменить изменения файла после HEAD. → Discard file changes after HEAD. (vendor API EN)

## meta.rs
- `list_available_file_extension`: Список доступных расширений файлов. → List of available file extensions. (vendor API EN)
- `list_available_r_dir`: Список доступных директорий для чтения. → List of directories available for reading. (vendor API EN)
- `list_available_w_dir`: Список доступных директорий для записи. → List of directories available for writing. (vendor API EN)
- `list_available_r_file`: Список доступных файлов для чтения. → List of files available for reading. (vendor API EN)
- `list_available_w_file`: Список доступных файлов для записи. → List of files available for writing. (vendor API EN)
- `list_available_shell_commands`: Список доступных shell команд. → List of available shell commands. (vendor API EN)

## terminal.rs
- `run_shell`: Запуск команды. → Run a command. (vendor API EN)
- `run_invoke`: Запуск команды invoke. → Run an invoke command. (vendor API EN)

## user.rs
- `ask_user`: Задать вопрос пользователю и получить ответ. → Ask the user a question and get the answer. (vendor API EN)
- `request_read_access_for`: Запрос у пользователя read-доступа к папке/файлу. → Request read access to a folder/file from the user. (vendor API EN)
- `request_write_access_for`: Запрос у пользователя write-доступа к папке/файлу. → Request write access to a folder/file from the user. (vendor API EN)
- `request_command_shell_execution`: Запрос у пользователя исполнения shell-команды. → Request permission from the user to execute a shell command. (vendor API EN)
