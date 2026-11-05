# Задача на рефакторинг: очистка кода перед финальным merge

## Цель рефакторинга:
Уменьшить объём кодовой базы перед финальным merge: удалить все unit-тесты native-крейтов, код и настройки, обслуживающие только legacy/обратную совместимость, а также ставшие ненужными зависимости и шаг CI. Логика работы продукта (CWD-first, `output_buff_size`, `deprecated-dir-patterns`, server, dump/resume) остаётся без изменений.

## Вносимые изменения:

### Изменение №1: Удалить все unit-тесты native-крейтов
- Что хотим изменить и почему это лучше: unit-тесты были нужны на этапе разработки phase-7; перед финальным merge они исключаются из кодовой базы. Продакшн-код при этом не меняется.
- Как есть сейчас: в native-крейтах есть `#[cfg(test)]`-модули, файл `crates/conf/src/load_tests.rs` и декларация тестового модуля в `crates/conf/src/lib.rs`.
- Как должно быть: весь тестовый код удалён. Конкретно:
  - Удалить файл `crates/conf/src/load_tests.rs` целиком.
  - В `crates/conf/src/lib.rs` удалить строку `#[cfg(test)] mod load_tests;`.
  - Удалить `#[cfg(test)] mod tests { ... }` целиком (со всеми `#[test]` / `#[tokio::test]`, тестовыми helper-функциями и тестовыми `use`) в файлах:
    - `crates/conf/src/glob.rs`
    - `crates/conf/src/template.rs`
    - `crates/tools/src/lib.rs`
    - `crates/tools/src/process.rs`
    - `crates/tools/src/terminal.rs`
    - `crates/tools/src/user.rs`
    - `crates/tools/src/fs/access.rs`
    - `crates/agent-loop/src/dump_settings.rs`
    - `crates/agent-loop/src/vendor_request.rs`
    - `crates/cli-harness/src/cli.rs`
    - `crates/cli-harness/src/session.rs`
    - `crates/cli-harness/src/server/config.rs`
  - Если после удаления тестов в каком-либо файле появляются неиспользуемые `use` / приватные helper-функции (которые использовались только тестами), удалить и их. Продакшн-функции и публичные API не трогать.
  - После правок в native-крейтах не должно остаться ни одного вхождения `#[cfg(test)]`, `#[test]`, `#[tokio::test]`.

### Изменение №2: Удалить legacy/backward-compat проверки в `conf`
- Что хотим изменить и почему это лучше: код, который существует только для обработки старых имён ключей, больше не нужен перед финальным merge.
- Как есть сейчас:
  - `crates/conf/src/load.rs`: функция `reject_legacy_model_keys` вызывается из `read_model_numbers` и отдельно отклоняет flat-ключи `token-limit`, `iteration-limit`, `iterations`, `USD-limit`, `rpm`, `tpm`, `rpd` на верхнем уровне model-файла.
  - `crates/conf/src/load.rs`: в `load_fs` есть специальная проверка `obj.contains_key(keys::EXTRA_SCOPE)` с сообщением «`extra-scope` was renamed to `available-files`».
  - `crates/conf/src/str/keys.rs`: объявлены константы `ITERATIONS` и `EXTRA_SCOPE`, используемые только этими проверками.
- Как должно быть:
  - Удалить функцию `reject_legacy_model_keys` и её вызов в `read_model_numbers`.
  - Удалить проверку `obj.contains_key(keys::EXTRA_SCOPE)` в `load_fs` вместе с текстом ошибки.
  - Удалить константы `ITERATIONS` и `EXTRA_SCOPE` из `crates/conf/src/str/keys.rs` (вместе с их doc-комментариями).
  - Обновить комментарии, упоминающие legacy-ключи:
    - в шапке `crates/conf/src/load.rs` убрать/переформулировать упоминание «Flat legacy keys ... are a load error»;
    - в `crates/conf/src/scan.rs` убрать/переформулировать упоминания «A legacy flat key ... skips that file only» и «Rejects legacy root keys».
  - Поведение после упрощения: model-файл без `limits` / `price` по-прежнему не загружается (ошибка `please define 'limits'...` / `please define 'price'...`); если `limits` и `price` заданы, любые дополнительные flat-ключи на верхнем уровне model-файла игнорируются. Ключ `extra-scope` внутри `tools.fs` игнорируется так же, как любой другой неизвестный ключ `tools.fs`. Это осознанное упрощение под финальный merge.

### Изменение №3: Убрать legacy-допущения в CI
- Что хотим изменить и почему это лучше: CI больше не должен поддерживать старые имена тегов и legacy-имя Windows-артефакта.
- Как есть сейчас в `.github/workflows/ci.yml`:
  - `on.push.tags` содержит и `'v*'`, и legacy `'harness-*'`.
  - Проверка имени артефакта использует regex `^harness-[0-9]+\.[0-9]+\.[0-9]+(-linux|-macos|\.exe|-windows\.exe)$`, где альтернатива `\.exe` — legacy Windows-имя.
- Как должно быть:
  - В `on.push.tags` оставить только `'v*'`; строку `- 'harness-*'` удалить.
  - Regex ужесточить до `^harness-[0-9]+\.[0-9]+\.[0-9]+(-linux|-macos|-windows\.exe)$` (альтернативу `|\.exe` удалить).
  - Поведение: `harness-<version>-windows.exe`, `harness-<version>-linux`, `harness-<version>-macos` принимаются; `harness-<version>.exe` и прочие нецелевые имена отклоняются.

### Изменение №4: Удалить шаг `Test` из CI
- Что хотим изменить и почему это лучше: после удаления unit-тестов шаг `cargo test --workspace` не проверяет ничего полезного и только тратит время CI.
- Как есть сейчас в `.github/workflows/ci.yml`:
  ```
  - name: Test
    run: cargo test --workspace
  ```
- Как должно быть: шаг удалён. Шаги `Clippy (deny warnings)` и `Build release` остаются без изменений.

### Изменение №5: Удалить неиспользуемые зависимости/фичи
- `crates/harness-api/Cargo.toml`: удалить зависимость `serde_json = "1"` и комментарий `# JSON used by contract tests`. В `harness-api/src/lib.rs` используется только `serde`; `serde_json` в крейте не используется.
- `crates/tools/Cargo.toml`: у `tokio` изменить features с `["rt", "macros"]` на `["rt"]` (макросы использовались только `#[tokio::test]`-тестами, которые удаляются).
- `crates/interactions/Cargo.toml`: у `tokio` изменить features с `["rt", "macros"]` на `["rt"]` и поправить комментарий: `# \`spawn_blocking\` for stdin reads and the async test runtime.` → `# \`spawn_blocking\` for stdin reads.`

## Проверка корректности рефакторинга:
- `cargo check --workspace` — без ошибок.
- `cargo clippy --workspace --all-targets -- -D warnings` — без предупреждений (CI-гейт должен остаться зелёным).
- `cargo test --workspace` — собирается и сообщает `0 tests` (сам вызов не обязателен для CI, но убедиться, что workspace собирается в test-профиле без ошибок).
- Поиском по native-крейтам убедиться, что не осталось `#[cfg(test)]`, `#[test]`, `#[tokio::test]`.
- Поиском убедиться, что в `crates/conf` не осталось `ITERATIONS`, `EXTRA_SCOPE`, `reject_legacy_model_keys`.
- В `.github/workflows/ci.yml` regex имени артефакта не содержит альтернативы `\.exe`, а `on.push.tags` не содержит `'harness-*'`.

## Scope
Минимально необходимые изменения:

- `crates/conf/src/lib.rs`
- `crates/conf/src/load_tests.rs` (удаление)
- `crates/conf/src/load.rs`
- `crates/conf/src/glob.rs`
- `crates/conf/src/template.rs`
- `crates/conf/src/scan.rs`
- `crates/conf/src/str/keys.rs`
- `crates/tools/src/lib.rs`
- `crates/tools/src/process.rs`
- `crates/tools/src/terminal.rs`
- `crates/tools/src/user.rs`
- `crates/tools/src/fs/access.rs`
- `crates/agent-loop/src/dump_settings.rs`
- `crates/agent-loop/src/vendor_request.rs`
- `crates/cli-harness/src/cli.rs`
- `crates/cli-harness/src/session.rs`
- `crates/cli-harness/src/server/config.rs`
- `crates/harness-api/Cargo.toml`
- `crates/tools/Cargo.toml`
- `crates/interactions/Cargo.toml`
- `.github/workflows/ci.yml`

Справочные материалы для чтения:

- `WiKi/todo/phase-7/pm-consulting report 3/report from refactoring manager.md`
- `WiKi/todo/phase-7/pm-consulting report 3/base.md` (решения №5 и №6 про legacy-артефакты и dump)
- `WiKi/todo/phase-7/pm-consulting report 3/tech.md`
- текущие исходники `crates/conf/src`, `crates/tools/src`, `crates/cli-harness/src`, `crates/agent-loop/src`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 11-code-cleanup-before-merge → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
