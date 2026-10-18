# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: `tools.fs` — применение `deprecated-dirs` / `deprecated-files` в инструментах
Научить инструменты файловой системы (и всё, что использует общий allow-list `Sandbox`) полностью игнорировать пути из `deprecated-dirs` / `deprecated-files`.

Зачем: временные файлы, логи и каталоги сборки не должны попадать ни в результаты поиска и листинга (контекст анализа), ни в операции чтения/записи/удаления агентом.

Предусловие: выполнена задача 2 (`FsSettings.available_files`, `deprecated_dirs`, `deprecated_files` уже есть в `conf`, конфликты разрешённого и устаревшего отсекаются при загрузке). Harness для этой задачи нужно пересобрать после задачи 2 — конфиг использует новую схему.

Ожидаемый результат: при `available-dirs: ["r:."]` и `deprecated-dirs: ["target"]`:
- `list_folder(".", 3)` не содержит `target` и его содержимого;
- `find_str`, `find_str_by_regex`, `find_file_by_str`, `find_files_by_regex` с `root="."` не заходят в `target`;
- `read_file("target/x.txt")`, `write_file`, `create_*`, `remove_*`, `move_*`, git-инструменты и `request_*_access_for` для пути внутри `target` возвращают ошибку «path is deprecated».

## Функциональные требования
### 1. Хранение
1. `Sandbox` (`crates/tools/src/fs/access.rs`) получает списки `deprecated_dirs` и `deprecated_files` (пути без префиксов). `Sandbox::from_fs(...)` принимает их из `FsSettings`; `FsModule::build` передаёт значения из конфигурации. Если модуля `fs` в конфигурации нет — списки пустые.
2. Сопоставление путей — то же, что для allow-list: `abspath` относительно текущей директории процесса. Для deprecated-директорий сопоставление **по границам компонентов** (`target` не должен блокировать `target2`); для deprecated-файлов — точное совпадение `abspath`. На Windows сравнение регистронезависимое. Поведение существующих allow-list не менять.

### 2. Запрет операций
1. `Sandbox::check_access(path, mode)` (оба режима `r` и `w`) **сначала** проверяет deprecated: путь равен deprecated-файлу или лежит внутри deprecated-директории → отказ, **даже если** путь разрешён через `available-dirs` / `available-files`.
2. Текст ошибки отличается от обычного «access denied» и однозначно говорит, что путь исключён конфигурацией и повторять попытку не нужно, например:
   `{mode}-access denied to {path} → path is deprecated (excluded by 'deprecated-dirs'/'deprecated-files' in tools.fs; do not use it)`
   Список deprecated-путей в тексте ошибки **не** перечислять.
3. Так как `check_access` общий, правило автоматически действует для `read_file`, `write_file`, `create_file`, `remove_file`, `move_file` (оба конца), `create_folder`, `remove_folder`, `move_folder`, `list_folder` (когда сам `path` deprecated), корня поиска и для git-инструментов. Для `move_*` проверяются и `src`, и `dst`.
4. Если корень поиска (`root`) или `path` для `list_folder` **содержит** deprecated-пути внутри — это не ошибка, они просто пропускаются (см. п. 3).

### 3. Обход и поиск
1. `list_folder` (`walk_folder` в `crates/tools/src/fs.rs`): не выводит deprecated-файлы и deprecated-директории, не спускается внутрь последних.
2. `find_str`, `find_str_by_regex`, `find_file_by_str`, `find_files_by_regex` (`collect_files` в `crates/tools/src/fs/search.rs`): deprecated-директории пропускаются **целиком без спуска**, deprecated-файлы не попадают ни в обработку, ни в счётчик `files observed` прогресса поиска.
3. Порядок вывода и формат результата не меняются.

### 4. Runtime-гранты (`user`)
1. `request_read_access_for` / `request_write_access_for` для deprecated-пути (или пути внутри deprecated-директории) отвечают отказом **до** вопроса пользователю: `read-access denied to '{path}': path is deprecated` (возвращается как обычный результат инструмента, как сейчас возвращаются отказы). Пользователя не беспокоить.
2. `Sandbox::grant_path_access` не должен добавлять deprecated-путь в allow-list (метод может вернуть признак отказа / `Result` — на усмотрение; существующие тесты поправить).
3. Дополнительные инструменты `list_deprecated_*` **не добавлять**.

### 5. Прочее
1. `list_available_r_dir` и аналоги выводят allow-list как раньше (без изменений).
2. Временные файлы самого harness (patch-файлы `apply_diff_patch` в каталоге сессии) пишутся напрямую через `std::fs`, а не через `Sandbox`, — не менять.
3. Описания tools (`descriptors`) не менять (справка — отдельные задачи 7–8).

## Scope
- `crates/tools/src/fs/access.rs`
- `crates/tools/src/fs.rs`
- `crates/tools/src/fs/search.rs`
- `crates/tools/src/user.rs`
- `crates/tools/src/lib.rs` (только если нужно для сборки `Sandbox`; тесты регистра)
- `crates/tools/src/git.rs` (только тесты, если изменилась сигнатура `Sandbox::from_fs`)

Не менять crate `conf` (задача 2 завершена).

## Проверка корректности решения
1. `cargo test --workspace`.
2. Unit-тесты в `tools` (tmp-каталоги, без сети):
   - `check_access` отказывает на deprecated-dir, на файл внутри неё, на deprecated-file — для `r` и `w`, даже при `w:`-разрешении родителя; сообщение содержит `deprecated`;
   - `target` не блокирует `target2` и `mytarget`;
   - `list_folder` с глубиной 3 не показывает deprecated-директорию и файл;
   - все четыре поисковых инструмента не находят совпадения внутри deprecated и не считают такие файлы в `files observed`;
   - поиск с `root` = deprecated-директория → ошибка `cannot find_str → ... deprecated`;
   - `move_file`: `dst` в deprecated → отказ, файл остался на месте;
   - `request_read_access_for` на deprecated-путь не обращается к stdin (в тесте — через проверку `UserModule` с deprecated sandbox, либо вынести проверку в отдельную функцию и тестировать её);
   - без deprecated-списков поведение идентично прежнему (существующие тесты зелёные).
3. Ручной шаг (не обязателен): запустить harness с `deprecated-dirs: ["target"]` и вызвать `find_str` в `.` — в прогрессе не должно быть файлов из `target`.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
