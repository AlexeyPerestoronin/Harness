# Результаты: 6-conf-load-pipeline-and-fs-validation

## Изменение №1: Единая read/parse-цепочка

**Было:** `load_with` и `load_server_config` содержали собственные копии
цепочки `read_to_string` → `vars::substitute` → `json5::from_str` →
`expect_object`, различаясь только обработкой ошибок.

**Стало:** внутренняя функция `read_root(conf_path) -> Result<Value, ConfError>`
выполняет всю цепочку с едиными сообщениями об ошибках. `load_with` и
`load_server_config` оставляют у себя только `normalize_abs` /
`ensure_conf_in_cwd` и ветвление результата:

- `load_with` передаёт `Value` в `load_from_value`;
- `load_server_config` после `fs::metadata` вызывает `read_root` и передаёт
  `Value` в `server_settings_from_value`, конвертируя ошибки в
  `ServerConfigState::Invalid`.

**Почему:** устранено дублирование и риск дрейфа порядка проверок и
сообщений об ошибках между двумя загрузчиками. Порядок
«`ensure_conf_in_cwd` до чтения файла» сохранён.

## Изменение №2: `ServerConfigState` — осознанное подавление clippy

**Было:** clippy выдавал `large_enum_variant` на `ServerConfigState`,
поскольку `Ok(ServerSettings)` весит ~648 байт.

**Стало:** на enum добавлен `#[allow(clippy::large_enum_variant)]` с
комментарием, фиксирующим решение: вызовы редки, горячей точки нет, а
упаковка в `Box` изменила бы публичный API и все pattern-match в
`cli-harness` (вариант 2 из отчёта Refactoring Manager).

**Почему:** предупреждение закрыто осознанно, без изменения поведения и
публичного API.

## Изменение №3: Контракт двойной валидации fs-путей

**Было:** `load_fs` и `fs_scope::check_conflicts`/`resolve_raw` валидировали
одни и те же `tools.fs` пути без явно зафиксированного контракта.

**Стало:**

- в `load_fs` добавлен комментарий, фиксирующий двойную валидацию и
  требование использовать `resolve_in_cwd` с каноническим текстом ошибки
  `resolves outside of cwd`;
- в `fs_scope::check_conflicts` и `resolve_raw` добавлены комментарии о том
  же контракте;
- в `load_tests.rs` добавлен тест, загружающий `tools.fs` с путём за
  пределами CWD и проверяющий, что сообщение содержит
  `resolves outside of cwd`.

**Почему:** сообщения об ошибках в двух точках валидации не должны
разъезжаться.

## Проверка

- `cargo test -p conf` — 14 passed, 0 failed.
- `cargo clippy -p conf --all-targets` — без предупреждений.
- `cargo check -p conf` — успешно.
