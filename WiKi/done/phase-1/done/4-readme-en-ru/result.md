# Результат: README.en.md и README.ru.md

- корневой документации не было → `README.en.md` и `README.ru.md` с одной структурой разделов (что это, сборка, CLI, конфиг, инструменты, CI) и ссылкой на другую языковую версию в шапке (без чтения исходников было непонятно, как собрать, запустить и сконфигурировать бинарь)
- отдельный `README.md` не создавался → только пара EN/RU, как в задаче (единый безымянный README не требовался)
- CLI сверен с `crates/harness/help.txt` → в обоих файлах xor `--prompt`/`--dump`/`--conf-new`, default `--conf` = `<process-cwd>/harness.json5`, плюс фактические `--help-tools` и `--version`
- конфиг/агенты сверены с `crates/conf` (`keys`, `AgentKind`) и шаблоном → `cwd`, `paths.log-folder`, `agent-setting`, `tools`; model json5: `vendor`/`name`/`api-key` (строка или путь); Google — обязательный `rpm`; известные агенты включают текущий `Claude` / `Claude Sonnet 5.5`
- tools сверены с текущими ключами, не с устаревшим `extra-scope` → модули `fs`/`git`/`terminal`/`user`; у `fs` поля `available-dirs`, `available-file-extensions`, `available-files` (`r:`/`w:`), `deprecated-dirs`/`deprecated-files`; нет `search`/`meta`; неизвестный ключ в `tools` — ошибка загрузки
- CI сверен с `.github/workflows/ci.yml` → push в `master`, Windows и Linux, `cargo test --workspace` + `cargo build --release --workspace`

Код приложения не менялся.
