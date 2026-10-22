# Результат: выбор агента из папки

## API загрузки
`conf::load(path)` только для файла; каталог падал бы на чтении как файла → `load` остаётся файловым (chooser не вызывается), каталог идёт через `conf::load_with(path, chooser)` (один вызов в `main.rs` вместо двухэтапного scan+load; для файла chooser не трогается, тесты `load` зелёные).

Generic `FnMut(&[AgentCandidate]) -> Result<usize, String>` вместо `&mut dyn FnMut` (статическая диспетчеризация, вызов `load_with(&path, agent_select::choose)` без обёртки).

## conf
`agent-setting.config` только файл → файл или каталог `.json5` в корне (без рекурсии, без чтения `api-key`/`rpm` до выбора) — список моделей в `agents/` без правки `harness.json5`.

Нет типа кандидата → `AgentCandidate { path, vendor, name }`. Сканирование в `crates/conf/src/scan.rs`. `AgentKind::from_vendor_name` общий для load и scan.

Путь без расширения не существует: `cannot read model file ...` → то же + `(expected a file or a directory)`.

Пустой каталог: IO/parse ошибка файла → `no agent config files found in '<dir>'` (+ `skipped '…'` если были).

`AgentSettings` без пути файла → поле `config_path` (лог `- agent config: …` до `session_dir`).

Комментарий `config` в шаблоне «только файл» → «path to an agent file or to a folder with agent files (you will be asked to choose)».

## harness
`start_session` → `conf::load(&conf)` → `conf::load_with(&conf, agent_select::choose)` (`--prompt` и `--dump` оба проходят выбор до создания сессии).

Интерактива в conf нет → `crates/harness/src/agent_select.rs`: список, 3 попытки, EOF/пустой ввод → `agent was not selected`; один кандидат → `Agent: vendor name (file)` без вопроса.

`log.md`: только `- agent: vendor model` → плюс `- agent config: <abs path>`.

`--help` / `--conf`: не сказано про каталог агентов → одна фраза, что `agent-setting.config` может быть папкой.

## Проверка
`cargo test --workspace`: 57 conf + 26 harness (+ 3 ignored live) + 38 tools.
