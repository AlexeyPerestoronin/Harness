# Результат: `tools.fs` — применение `deprecated-dirs` / `deprecated-files`

## Sandbox (`crates/tools/src/fs/access.rs`)
- `from_fs(dirs, files, ext)` → `from_fs(..., deprecated_dirs, deprecated_files)` (списки без префиксов из `FsSettings`; пустые, если модуля `fs` нет).
- `check_access` сначала denylist, даже при `w:`-разрешении родителя → отказ `{mode}-access denied to {path} → path is deprecated (excluded by 'deprecated-dirs'/'deprecated-files' in tools.fs; do not use it)` (список deprecated-путей в тексте не выводится).
- deprecated-dir: сравнение `abspath` по границам компонентов (`target` ⊄ `target2` / `mytarget`); deprecated-file: точный `abspath`; Windows регистронезависимо. Allow-list не трогал.
- `grant_path_access` → `Result`: deprecated-путь не добавляется в allow-list.
- `is_deprecated(path)` — общая проверка для обхода и `user`.

## Обход и поиск
- `list_folder` / `walk_folder`: не выводит deprecated-файлы и -директории, внутрь последних не спускается. Корень deprecated → ошибка `check_access`.
- `collect_files` (`find_str`, `find_str_by_regex`, `find_file_by_str`, `find_files_by_regex`): deprecated-dir пропускается целиком; deprecated-file не в обработке и не в `files observed`. `root` = deprecated → `cannot find_str → ... deprecated`.

## Runtime-гранты (`user`)
- `request_read_access_for` / `request_write_access_for` для deprecated-пути отказывают до stdin: `read-access denied to '{path}': path is deprecated` (обычный результат инструмента).

## Сборка
- `FsModule::build` передаёт `fs.deprecated_dirs` / `fs.deprecated_files`. `conf` не менялся. descriptors не менялись. `list_available_*` без изменений.

## проверка
- `cargo test --workspace` — ok (в т.ч. `check_access` r/w, `target` vs `target2`/`mytarget`, list_folder depth 3, четыре поиска + `files observed`, root deprecated, move_file dst, request_* без stdin, Windows case-insensitive).
