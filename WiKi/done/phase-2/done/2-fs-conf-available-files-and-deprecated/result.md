# Результат: `tools.fs` — `available-files` и `deprecated-*`

## conf
- `keys::EXTRA_SCOPE` (= `"extra-scope"`) как рабочий ключ → `keys::AVAILABLE_FILES` (= `"available-files"`); `EXTRA_SCOPE` оставлен только как старое имя для ошибки загрузки; добавлены `DEPRECATED_DIRS` / `DEPRECATED_FILES` (все имена ключей читаются из `keys`).
- `FsSettings.extra_scope` → `available_files`; добавлены `deprecated_dirs` / `deprecated_files` (симметрия с `available-*`, denylist без префиксов доступа).
- `load_fs`: отсутствие `deprecated-*` → ошибка `please define '…' in 'tools.fs' (it may be an empty list)`; не-массив → `'…' must be an array of strings`; пустая строка / `r:` / `w:` в denylist → ошибка (префикс — режим доступа, для запрета бессмыслен). Хелпер `required_string_list`, не `optional_string_list`.
- Ключ `extra-scope` в `tools.fs` → `'extra-scope' was renamed to 'available-files'` (старые конфиги не должны молча потерять доступ к файлам).
- После чтения `tools.fs` — лексическая проверка пересечений (без FS, без зависимости `conf`→`tools`): strip `r:`/`w:` у `available-*`, разворот относительно cwd процесса, `.`/`..`, `\`=`/`, без конечного разделителя, Windows регистронезависимо, сравнение по компонентам (`crates/conf` ⊄ `crates/conf2`). Конфликт: available dir совпадает с deprecated dir или лежит внутри неё; available file совпадает с deprecated file; available file внутри deprecated dir. Допустимо: deprecated внутри available (исключения вроде `r:.` + `target`). Все конфликты в одном сообщении `tools.fs: conflicting settings` с исходными строками, resolved-путями и причиной.
- Логика сравнения вынесена в `crates/conf/src/fs_scope.rs` (загрузка JSON ≠ сравнение путей).
- `--conf-new` шаблон: `extra-scope` → `available-files`, добавлены пустые `deprecated-dirs` / `deprecated-files` с комментариями (шаблон остаётся валидным json5).

## tools (только переименование)
- `Sandbox::from_fs(..., extra_scope, ...)` / `fs.extra_scope` → `available_files`. Поведение sandbox не менялось; `deprecated_*` инструменты не читают.

## live tests
- `test-settings.json5`: `extra-scope` → `available-files`, обязательные `deprecated-dirs: []`, `deprecated-files: []`.
- `agents_live_tests`: обращение к `available_files`.

## не сделано здесь
- Корневой `harness.json5` не трогал (см. `note.md`).
- `WiKi/todo/phase-1/**` не менялся.
- Применение `deprecated-*` в fs-инструментах — задача 3.

## проверка
- `cargo test --workspace` — ok (в т.ч. новые тесты conf: полный fs, пустые списки, missing keys, rename, prefix/empty, конфликты dirs/files/вложенность, несколько конфликтов сразу, допустимые overlapping, `\` vs `/`).
