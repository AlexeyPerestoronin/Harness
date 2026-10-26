# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: Копия `harness.json5` внутри dump-файла
В dump-файл, который пишется на каждой итерации цикла, добавлять полную копию конфигурации цикла (`harness.json5`), чтобы продолжение после прерывания не требовало отдельного файла конфигурации. Попытка задать `--conf` вместе с `--dump` — ошибка.

Зачем: сейчас `--dump` всё равно читает `harness.json5` (или default-путь). Если исходный conf удалили, перенесли или изменили, сессию нельзя продолжить тем же окружением инструментов и лимитов.

Предусловия: выполнена задача 1 (ключ `agent`, нет `paths`). Задача 2 (`--prompt-file`) желательна: xor в `help.txt` уже включает `--prompt-file` — не выкидывать его.

Ожидаемый результат: `dump.json` имеет вид
```json
{
  "settings": {
    "cwd": "...",
    "agent": { "config": "<absolute path of the chosen agent file>", "...": "..." },
    "tools": { "...": "..." }
  },
  "...": "vendor-specific session fields as today"
}
```
`harness.exe --dump <dump.json>` поднимает сессию из `settings` внутри файла, без `--conf`. `harness.exe --conf x.json5 --dump dump.json` завершается с ошибкой.

## Функциональные требования
### 1. Формат dump
1. Dump остаётся JSON (не json5). Вендорные поля **не унифицировать** (это не задача про независимый от агента dump): xAI по-прежнему пишет `conv_id` / `chat_id` / `usage_stats` / `messages`, Google — `usage_stats` / `history` / `pending_message`, Claude — `messages` / `usage`.
2. На верхнем уровне объекта появляется ключ `"settings"` — JSON-объект, копия **использованного** `harness.json5` **после** подстановки `vars`. Комментарии json5 не сохраняются (это JSON).
3. В копии поле `agent.config` (после задачи 1; не `agent-setting`) переписать на **абсолютный путь выбранного файла агента**. Даже если исходный `config` указывал на каталог, в dump должен быть файл — при продолжении вопрос «выберите агента» не задаётся.
4. Остальные пути в копии не резолвить специально: `cwd` уже абсолютный/как в файле; `tools.fs.*` резолвятся от process-cwd при следующей загрузке, как сейчас.
5. Ключ `"settings"` пишет **не** вендорный агент. `dump_session` агентов оставить как есть (пишут только свои поля). Ответственный — `AgentLoop` (или маленький модуль рядом, например `crates/harness/src/dump_settings.rs`):
   - после `agent.dump_session(path)` прочитать JSON-объект, вставить/перезаписать `settings`, записать обратно;
   - serde у dump-структур агентов неизвестные поля игнорирует (сейчас `deny_unknown_fields` нет — не добавлять), поэтому `reload_session` продолжит работать.
6. Старый dump **без** `settings`: ошибка `dump '<path>' has no settings; cannot continue without embedded configuration`. Фоллбека на `--conf` нет.

### 2. CLI
1. `--dump` **нельзя** сочетать с `--conf` (в любой форме, включая default). Сообщение: `--conf cannot be used with --dump (configuration is taken from the dump file)`.
2. Сейчас `parse_args` для `--dump` подставляет `default_conf_path()`, если `--conf` не указан. Это нужно убрать: при `--dump` поле conf = `None`.
3. `--dump` по-прежнему взаимоисключающий с `--prompt`, `--prompt-file` (если уже есть) и `--conf-new`. Один из них обязателен.
4. `CliAction::Start.conf` сменить на `Option<PathBuf>`: `Some` для `--prompt` / `--prompt-file`, `None` для `--dump`.
5. `help.txt` и README: `--dump` больше не сопровождается `[--conf <path>]`; явно написать, что конфигурация берётся из dump и `--conf` с `--dump` запрещён. Существующий `--prompt-file` в USAGE/RULES сохранить, если он уже описан.

### 3. Загрузка settings из dump
1. crate `conf` по-прежнему единственный, кто превращает json-объект настроек в `Settings`. Добавить функцию вида
   `pub fn load_from_value(root: serde_json::Value, conf_dir: &Path, chooser: F) -> Result<Settings, ConfError>`
   либо вынести общую часть из `load_with` так, чтобы чтение с диска и чтение из dump шли одним валидатором.
2. Для dump: `vars` **не** подставлять повторно (объект уже после substitution). `conf_dir` — родительский каталог абсолютного `agent.config` из dump (нужен только если внутри копии остались относительные пути к модели; после п. 1.3 `config` абсолютный, chooser не вызывается).
3. `main.rs` / `start_session`:
   - ветка `--prompt` / `--prompt-file`: как сейчас, `conf::load_with(&conf_path, agent_select::choose)`;
   - ветка `--dump`: прочитать dump как JSON, извлечь `settings`, вызвать `load_from_value`, затем `AgentLoop::new` + `start(..., source_dump)`.
4. Файл агента по абсолютному `agent.config` читается как сейчас (`api-key`, vendor/name). Если файла агента уже нет — обычная ошибка `cannot read model file`.
5. В `log.md` новой сессии, помимо `- continue from dump: <path>`, добавить `- settings: from dump`.

### 4. Что не делать
1. Не менять нейтральность/не-нейтральность сообщений, usage, native payload.
2. Не класть в dump файл агента и не инлайнить `api-key`.
3. Не менять схему `harness.json5` (это задача 1) и не добавлять skills/roles (задача 4).

## Scope
- `crates/harness/src/cli.rs`
- `crates/harness/src/main.rs`
- `crates/harness/src/agent_loop.rs`
- `crates/harness/src/dump_settings.rs` (создать при необходимости)
- `crates/harness/help.txt`
- `crates/conf/src/load.rs`
- `crates/conf/src/lib.rs` (экспорт + тесты `load_from_value`)
- `README.md`
- `README.ru.md`

Не менять: `xai.rs` / `google.rs` / `claude.rs` (кроме случая, если serde вдруг начнёт падать на лишнем ключе — тогда разрешить игнорировать `settings`), crate `tools`, шаблон `settings-template.json5`.

## Проверка корректности решения
1. `cargo test --workspace`. Живые API не вызывать.
2. CLI-тесты: `--dump d.json` → `conf = None`; `--conf x --dump d` → ошибка из п. 2.1; `--dump d --prompt p` по-прежнему xor; `--prompt p` без `--conf` по-прежнему default conf.
3. Unit-тесты на фикстурах JSON (без сети):
   - вставка `settings` в объект xAI/Google/Claude dump и обратно: вендорные поля не потеряны, `agent.config` — абсолютный файл;
   - dump без `settings` → ошибка из п. 1.6;
   - `load_from_value` на копии шаблона (после подстановки vars и подмены на tmp-файл агента) даёт тот же `cwd` / `log_folder` / лимиты, что `load_with` с диска.
4. Агентный `reload_session` на файле **с** ключом `settings` не падает (фикстура на каждый вендор).
5. Ручной шаг (не обязателен): прервать сессию, запустить `--dump <path>` без `--conf` и без `harness.json5` в process-cwd — сессия продолжается, пока файл агента на диске на месте.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
5. Если что-то непонятно, спроси!
