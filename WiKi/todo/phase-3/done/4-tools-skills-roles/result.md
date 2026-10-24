# result

- в `harness.json5` не было каталогов skills/roles → секции `tools.skills` / `tools.roles` (почему: длинные тексты не должны ехать в каждый запрос, агент подтягивает их по имени)
- ключей `TOOL_SKILLS` / `TOOL_ROLES` / `DESCRIPTION` / `FILE` не было → константы в `crates/conf/src/str/keys.rs` (почему: имена json5 живут только в `keys`)
- `ToolsSettings` без skills/roles → поля `Option<SkillsSettings>` / `Option<RolesSettings>` и типы `SkillEntry` / `RoleEntry` (почему: нет ключа = модуль выключен, `{}` = включён с пустым списком)
- `load_tools` не знал skills/roles → загрузка записей, резолв `file` от `conf_dir`, проверка `is_file` до цикла (почему: нет файла — цикл не стартует; сообщение дословно `error → skill-file '<abs>' doesn't find`)
- tool-модулей skills/roles не было → `crates/tools/src/skills.rs` и `roles.rs`, регистрация в реестре (почему: `get_skill_explanation` / `get_role_explanation` читают файл с диска без sandbox)
- шаблон `--conf-new` без skills/roles → пустые объекты с комментарием как добавить запись (почему: шаблон должен грузиться, без ссылок на несуществующие md)
- `help-user.txt` / README не упоминали каталоги → разделы skills/roles и одно предложение про запрос по имени (почему: оператор и агент должны знать имена tools и что `file` проверяется на старте)
