# result

## dump
- dump = только вендорные поля → dump = `"settings"` + вендорные поля (чтобы сессию можно было продолжить без `harness.json5`)
- `"settings"` не писал никто / агент писал бы сам → пишет `AgentLoop` через `dump_settings` после `dump_session` (вендорные агенты не трогаем)
- `agent.config` в копии = как в conf (файл или каталог, часто относительный) → абсолютный путь выбранного файла агента (при continue не спрашивать «выберите агента»)
- старый dump без `"settings"` = пытались читать `--conf` → ошибка `dump '<path>' has no settings; cannot continue without embedded configuration` (фоллбека на `--conf` нет)

## CLI
- `--dump` + optional `--conf` / default `harness.json5` → `--dump` без `--conf`; явное `--conf` с `--dump` = ошибка `--conf cannot be used with --dump (configuration is taken from the dump file)` (конфиг живёт в dump)
- `CliAction::Start.conf: PathBuf` → `Option<PathBuf>` (`Some` для `--prompt`/`--prompt-file`, `None` для `--dump`)
- USAGE/README: `harness.exe [--conf <path>] --dump ...` → `harness.exe --dump ...`; `--prompt-file` в xor сохранён

## загрузка
- `conf` умел только `load_with` с диска → общий валидатор `load_from_value` (без повторной подстановки `vars`); диск и dump идут одним путём
- `--dump` читал conf с диска → читает dump JSON, берёт `"settings"`, `conf_dir` = родитель абсолютного `agent.config`
- `log.md` при continue: только `- continue from dump: <path>` → плюс `- settings: from dump`
