# Результаты: CWD first в cli-harness и agent-loop

## Было

- `--cwd` работал как override json5-поля `cwd`; `harness.json5` содержал `cwd`.
- `--conf` по умолчанию для console-режима брался от process cwd, для server — от cwd; для резолва использовались две разные функции.
- `--dump` при возобновлении требовал внешний CWD (`--cwd` или process cwd); dump не хранил `"CWD"`.
- `agent-loop` писал в dump только `"settings"`; при возобновлении CWD вычислялся из `agent.config` через `conf_dir_of`.
- `server/config.rs` перезагрузка конфига (`reload_server_config` / `refresh_server_config`) читала CWD из состояния внутри себя, а `config_default` после создания шаблона всегда возвращал `valid: true`.
- Startup info сервера печатал `cwd` только при валидном конфиге.

## Стало

- `--cwd` задаёт только эффективный CWD; ключ `cwd` в `harness.json5` запрещён (ошибка загрузки).
- `default_conf_path(cwd: Option<&Path>)` используется и для `Start`, и для `Serve`; `default_conf_path_for_serve` удалён.
- `--cwd` с `--dump` — ошибка CLI; CWD при возобновлении берётся из верхнеуровневого `"CWD"` dump.
- `session::resolve_abs_path` резолвит относительные `--conf` / `--dump` от process cwd и нормализует результат; используется и для открытия файла, и для вывода.
- `start_new_session` печатает `cwd:` и `conf:` до выбора агента; `start_resume_session` печатает `cwd:` и `settings: from dump`, проверяет доступность CWD из dump до старта цикла.
- `AgentLoop` хранит `cwd` и при каждой записи dump вызывает `dump_settings::embed(dump_file, &settings, &cwd)`.
- `dump_settings` пишет/читает верхнеуровневые `"settings"` и `"CWD"`; `conf_dir_of` удалён.
- `server/config.rs`: `reload_server_config` и `refresh_server_config` принимают CWD параметром, `config_default` возвращает фактический `valid` после `refresh_server_config`.
- `help.txt` и корневой `harness.json5` обновлены; в `tools.fs` добавлен `deprecated-dir-patterns`.

## Почему

- Чтобы `harness.exe` работал как системная утилита: рабочая директория задаётся местом запуска или явным `--cwd`, а не конфигом.
- Чтобы возобновление из dump восстанавливало ту же CWD без дополнительных аргументов.
- Чтобы сервер валидировал и перезагружал конфиг ровно с той CWD, с которой был запущен, и честно сообщал `valid` для созданного шаблона.
