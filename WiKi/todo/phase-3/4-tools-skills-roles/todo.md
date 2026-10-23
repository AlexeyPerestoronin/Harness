# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: Модули tools `skills` и `roles`
Добавить в `harness.json5` секции `tools.skills` и `tools.roles` и два tool-модуля, через которые агент запрашивает полное описание навыка или роли из файла.

Зачем: краткие описания tools уходят в каждый запрос и стоят токенов. Навыки и роли — длинные тексты (как `WiKi/harness/roles/senior-developer.md`); агент должен подтягивать их по имени, только когда они нужны.

Предусловие: выполнена задача 1 (корень conf — `cwd` / `agent` / `tools`, без `paths` и `agent-setting`). Не зависит от dump и вендоров.

Ожидаемый результат: в конфигурации можно написать
```json5
{
    "cwd": "...",
    "agent": { ... },
    "tools": {
        "skills": {
            "python-programmer": {
                "description": "Применяется при программировании на Python",
                "file": "<path>/harness/skills/python-programmer.md"
            },
            "rust-programmer": {
                "description": "Применяется при программировании на Rust",
                "file": "<path>/harness/skills/rust-programmer.md"
            }
        },
        "roles": {
            "senior-developer": {
                "description": "Роль старшего разработчика: изменение кодовой базы проекта.",
                "file": "<path>/harness/roles/senior-developer.md"
            },
            "project-analyst": {
                "description": "Роль аналитика проекта: изучение кодовой базы для планирования.",
                "file": "<path>/harness/roles/project-analyst.md"
            }
        }
    }
}
```
Агент вызывает `get_skill_explanation({ "skill_name": "rust-programmer" })` / `get_role_explanation({ "role_name": "senior-developer" })` и получает содержимое соответствующего файла. Если файла нет на старте — цикл не начинается.

## Функциональные требования
### 1. Конфигурация
1. Ключи модулей: `keys::TOOL_SKILLS = "skills"`, `keys::TOOL_ROLES = "roles"`. Как у остальных модулей: нет ключа → модуль не регистрируется; неизвестный ключ в `tools` по-прежнему ошибка.
2. Значение `tools.skills` / `tools.roles` — объект. Ключи объекта — имена навыков/ролей. Имя: непустое, символы `A-Z a-z 0-9 _ - .`. Дубликаты невозможны (это ключи JSON). Одинаковые имена в skills и roles допустимы — это разные модули.
3. Каждая запись — объект с обязательными строками:
   - `description` (`keys::DESCRIPTION = "description"`) — непустая;
   - `file` (`keys::FILE = "file"`) — непустой путь к текстовому файлу.
   Лишние ключи в записи — ошибка `unknown key '<k>' in skill '<name>'` (аналогично для role).
4. Относительный `file` резолвится от каталога файла `harness.json5` (`conf_dir`), как `agent.config`. Абсолютный — как есть.
5. Проверка существования файлов — **до начала цикла**, на этапе `conf::load` / `load_from_value`. Нет файла или это каталог:
   - `error → skill-file '<file path>' doesn't find`
   - `error → role-file '<file path>' doesn't find`
   Текст сообщения **дословно** такой (включая `doesn't find`). Путь в сообщении — уже абсолютный.
6. Пустой объект `"skills": {}` допустим: модуль включён, записей нет, tool всё равно регистрируется.
7. Типы: `SkillEntry { description: String, file: PathBuf }`, `SkillsSettings { items: ... }`, то же для roles. Поля `ToolsSettings.skills: Option<SkillsSettings>`, `ToolsSettings.roles: Option<RolesSettings>`. Экспорт типов из `conf`.

### 2. Модули tools
1. Файлы:
   - `crates/tools/src/skills.rs` — навык;
   - `crates/tools/src/roles.rs` — роль.
   Регистрация в `crates/tools/src/lib.rs` (`Module::Skills` / `Module::Roles`), по образцу `user.rs`. Общий хелпер чтения файла можно вынести рядом, но публичные модули — два.
2. `ToolRegistry::build` включает модуль, если соответствующее `Option` — `Some`.
3. Дескрипторы (язык — английский, как остальные tools):
   - имя `get_skill_explanation`; description: что tool возвращает полный текст навыка по имени; в description **встроить список** `name — description` из conf (чтобы агент знал, какие имена есть). Если список пуст — явно написать `none configured`.
   - параметры: `{"type":"object","properties":{"skill_name":{"type":"string"}},"required":["skill_name"]}`.
   - симметрично `get_role_explanation` / `role_name`.
4. Вызов:
   - аргумент — точное имя (регистр значим);
   - успех: `CallResult::plain(<содержимое файла как есть, без обёртки>)`;
   - неизвестное имя: `Err("cannot get skill explanation → unknown skill 'x'; available: a, b, c")` (для role — `role` / `get role explanation`);
   - файл пропал после старта: `Err("cannot get skill explanation → cannot read '<path>': ...")`.
5. Модули **не** ходят в `Sandbox` / fs-allow-list: путь задан владельцем conf, не агентом. Не использовать `read_file` из fs.
6. В рантайме повторно читать файл с диска (можно править md без пересборки). Не `include_str!`.

### 3. Шаблон и справка
1. `settings-template.json5`: добавить `tools.skills` и `tools.roles` как **пустые объекты** с комментарием, как добавить запись (`description` + `file`, путь относительно каталога conf). Не ссылаться на несуществующие md в репозитории — шаблон должен успешно проходить `conf::load` после подстановки vars (проверка файлов: пустой объект файлов не требует).
2. Тест согласованности ключей: `TOOL_SKILLS`, `TOOL_ROLES`, `DESCRIPTION`, `FILE`.
3. `crates/tools/help-user.txt`: разделы `skills` и `roles` (конфигурация, имена tools, что `file` проверяется на старте). В списке Known modules добавить `skills`, `roles`.
4. README: одно предложение, что агент может запросить skill/role по имени.

### 4. Что не делать
1. Не инжектить тексты skills/roles в системный промпт и не менять `AgentLoop`.
2. Не создавать каталоги `WiKi/harness/skills` и новые md, кроме tmp-файлов в тестах. Существующий `WiKi/harness/roles/senior-developer.md` можно использовать в ручной проверке, в шаблон его не хардкодить.
3. Не добавлять `list_available_skills` / `list_available_roles` — список уже в description tool.

## Scope
- `crates/tools/src/skills.rs` (создать)
- `crates/tools/src/roles.rs` (создать)
- `crates/tools/src/lib.rs`
- `crates/tools/help-user.txt`
- `crates/conf/src/str/keys.rs`
- `crates/conf/src/types.rs`
- `crates/conf/src/load.rs`
- `crates/conf/src/lib.rs` (тесты загрузки и отсутствия файла)
- `crates/conf/settings-template.json5`
- `README.md`
- `README.ru.md`

Не менять: агентов, CLI, dump, `vendor_request`.

## Проверка корректности решения
1. `cargo test --workspace`.
2. Тесты `conf` (tmp-файлы):
   - валидные skills+roles → `settings.tools.skills/roles` заполнены абсолютными путями;
   - нет файла → дословное сообщение из п. 1.5;
   - относительный `file` резолвится от `conf_dir`;
   - нет ключа `skills` → `None`, модуль выключен;
   - `"skills": {}` → `Some` с пустым списком;
   - шаблон по-прежнему грузится.
3. Тесты `tools`:
   - реестр: `skills: Some` → в `list()` есть `get_skill_explanation`; `None` — нет;
   - вызов с известным именем возвращает содержимое tmp-файла;
   - неизвестное имя — ошибка со списком available;
   - description дескриптора содержит имена из conf.
4. Ручной шаг (не обязателен): включить `"roles": { "senior-developer": { "description": "...", "file": "WiKi/harness/roles/senior-developer.md" } }` и попросить агента вызвать `get_role_explanation`.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
5. Если что-то непонятно, спроси!
