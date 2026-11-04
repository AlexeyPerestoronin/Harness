# Phase-7, задача 1: CWD first + deprecated-dir-patterns — базовый отчёт

## Было

- `harness.json5` содержал обязательный корневой ключ `cwd`; относительный `cwd` резолвился от `conf_dir`.
- `--cwd` был override значения из json5.
- Относительные пути конфига резолвились по-разному:
  - `agent.config`, `skills.*.file`, `roles.*.file` — от `conf_dir`;
  - `tools.fs.*`, `tools.terminal.cwd-list` — от `settings.cwd`;
  - `api-key` в model-файлах — от process cwd;
  - `server.tls-cert` / `server.tls-key` — без резолва.
- Корневые ключи `harness.json5` не валидировались.
- `tools.fs` поддерживал только точные `deprecated-dirs` / `deprecated-files`, без glob-паттернов.
- Существовала публичная `conf::load_model`.

## Стало

- `cwd` полностью исключён из json5 и является рантайм-значением, которое передаёт вызывающий код.
- Новые сигнатуры в `conf::load`:
  - `load(conf_path, cwd)`
  - `load_with(conf_path, cwd, chooser)`
  - `load_from_value(root, cwd, chooser)`
  - `load_server_config(conf_path, cwd)`
- `conf_path` приводится к абсолютному нормализованному виду и проверяется через `ensure_conf_in_cwd` до чтения файла.
- Добавлен общий `resolve_in_cwd(raw, cwd)`: относительный путь — `cwd.join(raw)`, абсолютный — как есть; результат нормализуется и проверяется на нахождение внутри `cwd` по компонентам (на Windows — регистронезависимо).
- Через `resolve_in_cwd` проходят:
  - `agent.config`;
  - `tools.fs.available-dirs` / `available-files` / `deprecated-dirs` / `deprecated-files` (для allow-строк — после снятия `r:` / `w:`);
  - `tools.terminal.cwd-list`;
  - `tools.skills.<name>.file` и `tools.roles.<name>.file`;
  - `agent.log-folder`.
- Осознанные исключения сохранены:
  - `server.tls-cert` / `server.tls-key` — относительный путь от `cwd`, без проверки «внутри CWD»;
  - `api-key` в model-файлах — относительный путь от `cwd`, без проверки «внутри CWD»;
  - `tools.git.executable` — не резолвится, передаётся в `Command::new` как есть.
- Корневая валидация `harness.json5`: допустимы только `vars`, `server`, `agent`, `tools`; `cwd` — отдельное сообщение об ошибке; неизвестный ключ — ошибка загрузки.
- Добавлен `tools.fs.deprecated-dir-patterns` (`Vec<String>`, default `[]`) и модуль `glob`:
  - `glob::regex_source(pattern)` транслирует строгий glob `**/<component>` в regex;
  - `*` → `[^/]*`, прочие символы экранируются как литералы;
  - итог: `(?:^|/)component(?:/|$)`.
- `load_fs` валидирует и компилирует каждый паттерн через `glob::regex_source` + `Regex::new`.
- Удалена публичная `conf::load_model` и её re-export.
- Обновлён `settings-template.json5`:
  - убраны `cwd` и `vars.workspace`;
  - `vars.agent-config` = `harness/agents/grok-4.6.json5`;
  - добавлен `deprecated-dir-patterns: []`;
  - обновлены комментарии о CWD и glob.

## Почему

- `cli-harness` должен запускаться как системная утилита: рабочая директория задаётся внешним контекстом запуска, а не файлом конфига.
- Единый источник истины для CWD — процесс запуска; это убирает неоднозначность резолва относительных путей.
- Строгий glob-подмножество `**/<component>` даёт пользователю гибкий запрет fs-доступа (`**/log`, `**/done`, `**/note-*`) без поддержки произвольных regex.
- Ранняя валидация и компиляция паттернов в `conf` гарантирует, что невалидный конфиг не дойдёт до рантайма.

## Проверка

- `cargo test -p conf` — 13 тестов проходят.
- `cargo build -p conf` — собирается без ошибок и предупреждений.

---

# Исправление ошибок компиляции после CWD-first refactoring

## Было

Крейт `conf` был переведён на CWD-first API, но вызывающий код `cli-harness` остался на старых сигнатурах, из-за чего workspace не собирался:

- `conf::load_server_config(&path)` — не хватало аргумента `cwd`;
- `conf::load_from_value(settings_value, &conf_dir, cwd, |_| ...)` — старый набор аргументов;
- `--cwd` по-прежнему применялся после загрузки конфига и резолвился относительно `conf_dir`, что противоречило модели CWD first.

## Стало

Обновлены call-sites `cli-harness`:

- `main.rs`:
  - `start_new_session` перед чтением конфига вычисляет эффективный `cwd` через `session::resolve_cwd` и передаёт его в `conf::load_server_config`;
  - убран пост-загрузочный `settings.cwd = ...`;
  - `start_resume_session` передаёт в `conf::load_from_value` эффективный `cwd`, а не `conf_dir`.
- `server/mod.rs`:
  - `serve` вычисляет эффективный `cwd` до загрузки конфига и передаёт его в `conf::load_server_config`;
  - убрано пост-загрузочное переопределение `settings.cwd`;
  - `LoadedServerConfig` получил поле `cwd`.
- `server/config.rs`:
  - все вызовы `load_server_config` получают `cwd` из `LoadedServerConfig`;
  - `validate_config_text` принимает `cwd`, а временный файл по-прежнему создаётся рядом с целевым конфигом.
- `server/state.rs`:
  - в `LoadedServerConfig` добавлено поле `cwd`.
- `session.rs`:
  - `resolve_cwd_override(raw, conf_dir)` заменён на `resolve_cwd(raw)` — `--cwd` резолвится относительно process cwd; при отсутствии `--cwd` берётся process cwd.
- `cli.rs` и `help.txt`:
  - обновлены комментарии и справка: `--cwd` — это workspace root, резолвится от process cwd; `harness.json5` больше не содержит ключ `cwd`; `--conf` обязан находиться внутри итогового cwd.

## Почему

CWD-first модель требует, чтобы вызывающий код определял рабочую директорию до загрузки конфига и передавал её явно. Конфиг больше не может ни задавать `cwd`, ни влиять на его резолв. После изменения публичного API `conf` необходимо было привести единственного потребителя — `cli-harness` — к новым сигнатурам и новой семантике `--cwd`.

## Проверка

- `cargo check --workspace` — без ошибок и warning-гов.
- `cargo test -p conf` — 13 тестов проходят.
- `cargo test -p cli-harness` — 0 тестов, компилируется и выполняется.
