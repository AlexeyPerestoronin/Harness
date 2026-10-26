# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: `tools.fs` — переименование `extra-scope` и параметры `deprecated-dirs` / `deprecated-files` (конфигурация и валидация)
В секции `tools.fs` файла `harness.json5`:
1. переименовать `extra-scope` → `available-files`;
2. добавить обязательные параметры `deprecated-dirs` и `deprecated-files`;
3. при загрузке конфигурации проверять пересечения «разрешённого» и «устаревшего» и прерывать программу с подробной ошибкой.

Зачем: `deprecated-*` — перечень путей (временные файлы, логи, каталоги сборки), которые не должны участвовать в операциях с файловой системой и попадать в контекст анализа. Имя `available-files` симметрично `available-dirs`. **Эта задача — только конфигурация (crate `conf`) и валидация.** Применение `deprecated-*` в инструментах — отдельная задача 3, здесь инструменты их не используют.

Ожидаемый результат:
```json5
"fs": {
    "available-dirs": ["r:."],
    "available-file-extensions": [],
    "available-files": ["w:README.md"],
    "deprecated-dirs": ["target", ".log"],
    "deprecated-files": ["Cargo.lock"]
}
```
Конфигурация загружается в `FsSettings { available_dirs, available_file_extensions, available_files, deprecated_dirs, deprecated_files }`; при конфликте `conf::load` возвращает ошибку, `harness.exe` печатает её и завершается с кодом 1 до старта цикла.

## Функциональные требования
### 1. Ключи и типы
1. `crates/conf/src/str/keys.rs`: `EXTRA_SCOPE` → `AVAILABLE_FILES = "available-files"`; новые `DEPRECATED_DIRS = "deprecated-dirs"`, `DEPRECATED_FILES = "deprecated-files"`. Все остальные места читают имена только из `keys` (правило модуля).
2. `crates/conf/src/types.rs`: `FsSettings.extra_scope` → `available_files`; новые поля `deprecated_dirs: Vec<String>`, `deprecated_files: Vec<String>`; докстринги обновить.
3. Старое имя `extra-scope` **не поддерживается**: если ключ присутствует в `tools.fs` — ошибка `'extra-scope' was renamed to 'available-files'` (иначе старые конфиги молча потеряют доступ к файлам).

### 2. Обязательность и формат
1. `deprecated-dirs` и `deprecated-files` обязательны, если в `tools` есть модуль `fs`. Отсутствие → ошибка вида `please define 'deprecated-dirs' in 'tools.fs' (it may be an empty list)`. Пустой массив допустим.
2. Значение — массив строк; иначе ошибка (`'deprecated-dirs' must be an array of strings`). Нужен отдельный хелпер `required_string_list` (существующий `optional_string_list` для этих ключей не использовать).
3. Элементы `deprecated-*` — пути **без** префиксов `r:` / `w:`. Элемент с таким префиксом → ошибка с объяснением (префикс означает режим доступа и для запрета не имеет смысла).
4. Пустая строка в `deprecated-*` → ошибка.
5. `available-files` остаётся необязательным (как был `extra-scope`), формат элементов прежний (`r:` / `w:` префикс).

### 3. Нормализация путей для сравнения
1. Из элементов `available-*` убрать префикс `r:` / `w:`.
2. Сравнение **лексическое**, без обращения к файловой системе (пути могут не существовать): относительный путь разворачивается относительно текущей директории процесса (так же делает `abspath` в `crates/tools/src/fs/access.rs`; `conf` не должен зависеть от `tools` — написать небольшой собственный хелпер в `conf`), нормализуются `.` / `..`, разделители `\` и `/` считаются равными, конечный разделитель игнорируется. На Windows сравнение регистронезависимое, на остальных ОС — регистрозависимое.

### 4. Правила пересечения (любое нарушение — ошибка загрузки)
Пусть нормализованные пути сравниваются по границам компонентов (`crates/conf` не является предком `crates/conf2`).
1. `available-dirs` ∩ `deprecated-dirs`: ошибка, если разрешённая директория **совпадает** с устаревшей **или лежит внутри** устаревшей (разрешение было бы полностью «мёртвым»).
2. `available-files` ∩ `deprecated-files`: ошибка при совпадении путей.
3. `available-files` внутри `deprecated-dirs`: ошибка (то же «мёртвое» разрешение).
4. **Допустимо** (не ошибка): устаревшая директория/файл лежит внутри разрешённой директории — это штатный способ исключений, например `available-dirs: ["r:."]` + `deprecated-dirs: ["target"]`.
5. Сообщение об ошибке собирает **все** найденные конфликты сразу, по строке на каждый, в формате:
   ```
   tools.fs: conflicting settings
     - 'available-dirs' entry 'w:crates/conf' (resolved: C:/MyLife/Harness/crates/conf) lies inside 'deprecated-dirs' entry 'crates' (resolved: C:/MyLife/Harness/crates)
   fix: remove the entry from one of the lists or narrow 'deprecated-dirs'
   ```
   (в тексте должны быть: имена обоих параметров, исходные строки из файла, нормализованные пути, причина — совпадение / вложенность).
6. Проверка выполняется в `conf::load` после чтения `tools.fs`, до возврата `Settings`.

### 5. Ровно необходимые правки вне `conf`
1. `crates/tools/src/fs.rs`, `crates/tools/src/fs/access.rs`: только переименование использования `extra_scope` → `available_files` (аргумент `Sandbox::from_fs`, обращение `fs.available_files`, докстринги/комментарии). Поведение инструментов не менять. Поля `deprecated_*` в `tools` пока не использовать.
2. `crates/harness/src/agents_live_tests/mod.rs` и `test-settings.json5`: переименование ключа и добавление обязательных `deprecated-dirs: []`, `deprecated-files: []`.
3. Корневой `harness.json5` (локальный файл пользователя): `extra-scope` → `available-files`, добавить `"deprecated-dirs": ["target", ".log"]`, `"deprecated-files": []`.
4. `crates/conf/src/template.rs` (генерация `--conf-new`): минимальная правка, чтобы шаблон оставался валидным — переименовать ключ и добавить два новых параметра с комментариями. Полная переработка шаблона — задача 4, здесь не делать.

## Scope
- `crates/conf/src/str/keys.rs`
- `crates/conf/src/types.rs`
- `crates/conf/src/load.rs`
- `crates/conf/src/template.rs` (минимально)
- `crates/conf/src/lib.rs` (тесты)
- `crates/tools/src/fs.rs`, `crates/tools/src/fs/access.rs` (только переименование)
- `crates/harness/src/agents_live_tests/mod.rs`, `crates/harness/src/agents_live_tests/test-settings.json5`
- `harness.json5` (корень)

Не менять `WiKi/todo/phase-1/**` (исторические конфиги).

## Проверка корректности решения
1. `cargo test --workspace`.
2. Тесты в `crates/conf/src/lib.rs` (временные каталоги, без сети):
   - полный `tools.fs` с новыми ключами загружается, поля `FsSettings` заполнены;
   - отсутствие `deprecated-dirs` / `deprecated-files` → ошибка с подсказкой про пустой список;
   - пустые массивы допустимы;
   - `extra-scope` → ошибка про переименование;
   - префикс `r:` / `w:` в `deprecated-*` → ошибка; пустая строка → ошибка;
   - конфликты: dirs=dirs (совпадение и вложенность), files=files, file внутри deprecated-dir; сообщение содержит оба параметра и исходные строки;
   - несколько конфликтов → все перечислены в одном сообщении;
   - допустимые случаи: `r:.` + `deprecated-dirs: ["target"]`; `r:crates` + `deprecated-files: ["crates/a.txt"]`; `crates/conf` vs `crates/conf2` — не конфликт;
   - `\` и `/` равнозначны (`crates\conf` vs `crates/conf`).
3. Ручная проверка: `harness.exe --conf-new <tmp>.json5` создаёт файл, который проходит json5-разбор и содержит новые ключи.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
