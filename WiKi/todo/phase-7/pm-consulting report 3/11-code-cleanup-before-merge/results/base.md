# Code cleanup before final merge — base

## Было

- В native-крейтах (`conf`, `tools`, `agent-loop`, `cli-harness`) находились `#[cfg(test)]`-модули, файл `crates/conf/src/load_tests.rs` и декларация `mod load_tests;`.
- `crates/conf/src/load.rs` содержал legacy-проверки:
  - `reject_legacy_model_keys` отклоняла flat-ключи верхнего уровня model-файла;
  - `load_fs` отдельно отклоняла `extra-scope` с сообщением о переименовании.
- `crates/conf/src/str/keys.rs` хранил константы `ITERATIONS` и `EXTRA_SCOPE`, используемые только этими legacy-проверками.
- Комментарии в `crates/conf/src/load.rs` и `crates/conf/src/scan.rs` упоминали legacy flat-ключи.
- В `.github/workflows/ci.yml`:
  - `on.push.tags` содержал legacy-тег `'harness-*'`;
  - regex имени артефакта принимал legacy Windows-имя `harness-<version>.exe`;
  - выполнялся шаг `Test: cargo test --workspace`.
- Зависимости:
  - `crates/harness-api/Cargo.toml` содержал неиспользуемый `serde_json`;
  - `crates/tools/Cargo.toml` и `crates/interactions/Cargo.toml` включали tokio-фичу `macros`, нужную только `#[tokio::test]`-тестам.

## Стало

- Из native-крейтов удалён весь unit-test код:
  - удалён `crates/conf/src/load_tests.rs`;
  - из `crates/conf/src/lib.rs` удалена строка `#[cfg(test)] mod load_tests;`;
  - удалены `#[cfg(test)] mod tests { ... }` в `conf/src/glob.rs`, `conf/src/template.rs`, `tools/src/lib.rs`, `tools/src/process.rs`, `tools/src/terminal.rs`, `tools/src/user.rs`, `tools/src/fs/access.rs`, `agent-loop/src/dump_settings.rs`, `agent-loop/src/vendor_request.rs`, `cli-harness/src/cli.rs`, `cli-harness/src/session.rs`, `cli-harness/src/server/config.rs`.
- В `conf` удалены legacy-проверки:
  - `reject_legacy_model_keys` и её вызов в `read_model_numbers`;
  - проверка `extra-scope` в `load_fs`;
  - константы `ITERATIONS` и `EXTRA_SCOPE`.
- Комментарии в `load.rs` и `scan.rs` больше не упоминают legacy-ключи.
- CI:
  - `on.push.tags` оставлен только `'v*'`;
  - regex имени артефакта ужесточён до `^harness-[0-9]+\.[0-9]+\.[0-9]+(-linux|-macos|-windows\.exe)$`;
  - шаг `Test` удалён.
- Зависимости:
  - `serde_json` удалён из `harness-api`;
  - `tokio` в `tools` и `interactions` переведён на `features = ["rt"]`.

## Почему

- Unit-тесты native-крейтов были нужны только на этапе разработки phase-7; перед финальным merge они исключаются из кодовой базы без изменения продакшн-логики.
- Legacy-обработка старых имён ключей и артефактов больше не нужна: финальный merge закрепляет только новые имена и поведение.
- Шаг `cargo test --workspace` после удаления тестов не проверяет ничего полезного и только тратит время CI.
- Неиспользуемые зависимости и tokio-фичи `macros` удаляются для уменьшения объёма зависимостей.
