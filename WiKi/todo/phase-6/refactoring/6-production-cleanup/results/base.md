# Production cleanup — базовое описание изменений

## Было

- `#[cfg(test)]`-модули и тестовые функции находились в крейтах:
  `logger`, `interactions`, `auth`, `harness-api`, `conf`, `tools`,
  `agent-loop`, `cli-harness`, `web-harness`.
- `crates/cli-harness/Cargo.toml` содержал `[dev-dependencies] tower`;
  `crates/tools/Cargo.toml` содержал `[dev-dependencies] async-trait`.
- В `crates/logger` существовали `DoubleLogger` и `NullLogger`, оставшиеся
  от старого бинарного крейта и тестов.
- В `crates/cli-harness/src/server/state.rs` поле `LoadedServerConfig::path`
  было помечено устаревшим `#[allow(dead_code)]`, хотя реально использовалось.

## Стало

- Из всех production-крейтов удалены `#[cfg(test)]`-модули и тестовые функции.
- Удалены dev-dependency секции из `cli-harness` и `tools`.
- Из `crates/logger` удалены `double.rs`, `null.rs` и их реэкспорты;
  модульная документация обновлена: остались контракт `Logger` и `FileLogger`.
- Убран obsolete `#[allow(dead_code)]` у `LoadedServerConfig::path`.

## Почему

- Production-сборка не должна содержать тестовый код и test-only зависимости.
- Мёртвый код (`DoubleLogger`, `NullLogger`) не имеет production-потребителей
  после удаления старого бинарного крейта и тестов.
- Атрибут `#[allow(dead_code)]` подавлял предупреждение для реально
  используемого поля и маскировал бы будущие проблемы.

## Проверка

- `cargo build --workspace` — успех, без warnings.
- `cd crates/web-harness && trunk build --release` — успех.
- Поиск по `crates` не находит `#[cfg(test)]` и `[dev-dependencies]`.
- `cargo tree -p cli-harness --depth 1` не содержит `tower`;
  `cargo tree -p tools --depth 1` не содержит `async-trait`.
