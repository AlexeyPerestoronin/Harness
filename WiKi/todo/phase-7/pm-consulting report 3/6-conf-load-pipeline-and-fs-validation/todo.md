# Задача на рефакторинг: Крейт conf — единая read/parse-цепочка и осознанное закрытие clippy

## Цель рефакторинга:
Устранить накопленное дублирование в крейте `conf`: одинаковая цепочка чтения/парсинга в `load_with` и `load_server_config`, а также задокументировать и зафиксировать тестом общий контракт валидации fs-путей в `load_fs` и `check_conflicts`. Попутно осознанно закрыть clippy-предупреждение `large_enum_variant` на `ServerConfigState` без изменения публичного API.

## Вносимые изменения:

### Изменение №1: Общая read/parse-цепочка в `crates/conf/src/load.rs`
- Что хотим изменить и почему это лучше: сейчас `load_with` и
  `load_server_config` повторяют одинаковые шаги
  `normalize_abs` → `ensure_conf_in_cwd` → `read_to_string` →
  `vars::substitute` → `json5::from_str` → `expect_object`, различаясь только
  обработкой ошибок (`Err` против `ServerConfigState::Invalid`). Дублирование
  создаёт риск дрейфа порядка проверок и сообщений об ошибках.
- Как есть сейчас: обе функции содержат собственную копию цепочки.
- Как должно быть: выделить внутреннюю функцию (например,
  `fn read_root(conf_path: &Path) -> Result<Value, ConfError>`), которая
  выполняет `read_to_string` → `vars::substitute` → `json5::from_str` →
  `expect_object` с едиными сообщениями об ошибках. `load_with` и
  `load_server_config` оставляют у себя только `normalize_abs` /
  `ensure_conf_in_cwd` и ветвление результата:
  - `load_with` передаёт `Value` в `load_from_value`;
  - `load_server_config` сначала делает `ensure_conf_in_cwd`, затем
    `fs::metadata` для `ServerConfigState::Missing`, затем вызывает
    `read_root` и передаёт результат в `server_settings_from_value`,
    конвертируя ошибки валидации в `ServerConfigState::Invalid`.
  Порядок проверок «`ensure_conf_in_cwd` до чтения файла» должен сохраниться.

### Изменение №2: `ServerConfigState` — осознанное подавление clippy
- В `crates/conf/src/types.rs` на enum `ServerConfigState` добавить
  `#[allow(clippy::large_enum_variant)]` с комментарием, объясняющим решение:
  `Ok(ServerSettings)` крупный (~648 байт), но `load_server_config`
  вызывается редко и не является горячей точкой; упаковка в `Box` изменила
  бы публичный API и все pattern-match в `cli-harness`, поэтому осознанно
  оставляем как есть (вариант 2 из отчёта Refactoring Manager).
- Поведение и публичный API не меняются.

### Изменение №3: Контракт двойной валидации fs-путей
- В `crates/conf/src/load.rs` (функция `load_fs`) и
  `crates/conf/src/fs_scope.rs` (функция `check_conflicts`/`resolve_raw`)
  добавить комментарии, явно фиксирующие, что обе точки валидации должны
  использовать один и тот же `resolve_in_cwd` и одинаковый текст ошибки
  «resolves outside of cwd», чтобы сообщения не разъезжались.
- Добавить в `crates/conf/src/load_tests.rs` тест, который загружает
  `tools.fs` с путём, выходящим за пределы CWD, и проверяет, что сообщение
  об ошибке содержит канонический текст `resolves outside of cwd`
  (из `resolve_in_cwd`), независимо от того, какая из точек валидации
  сработала.

## Проверка корректности рефакторинга:
- `cargo test -p conf` — все тесты проходят, включая новые.
- `cargo clippy -p conf --all-targets` — предупреждение `large_enum_variant`
  больше не выдаётся.
- `cargo check -p conf` — без ошибок.
- Публичный API `conf` не изменён: `load`, `load_with`, `load_from_value`,
  `load_server_config`, `ServerConfigState` сохраняют сигнатуры.

## Scope
Минимально необходимые изменения:

- `crates/conf/src/load.rs`
- `crates/conf/src/types.rs`
- `crates/conf/src/fs_scope.rs`
- `crates/conf/src/load_tests.rs`

Справочные материалы для чтения:

- `WiKi/todo/phase-7/pm-consulting report 3/report from refactoring manager.md`
- текущие исходники `crates/conf`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 6-conf-load-pipeline-and-fs-validation → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
