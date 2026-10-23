# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: CLI-флаг `--prompt-file`
Добавить возможность задавать стартовый промпт из файла: `harness.exe --prompt-file <path_to_prompt>`. По смыслу это тот же запуск, что `--prompt`, только текст читается из файла.

Зачем: длинные промпты (шаблоны задач, `todo.md`) неудобно передавать в кавычках командной строки, особенно в Windows.

Ожидаемый результат: `harness.exe [--conf <path>] --prompt-file <file>` стартует сессию с содержимым `<file>` в качестве пользовательского промпта; `--prompt` и `--prompt-file` вместе недопустимы.

Независимость: не зависит от задачи 1 (схема `agent`). Не трогать dump и встраивание settings.

## Функциональные требования
### 1. Разбор CLI
1. Новый флаг `--prompt-file <path>` в `crates/harness/src/cli.rs`. Формы `--prompt-file <path>` и `--prompt-file=<path>`, как у остальных флагов (`take_value`). Короткого алиаса нет.
2. Взаимоисключение расширяется: ровно один из `--prompt`, `--prompt-file`, `--dump`, `--conf-new` обязателен. Сообщение об ошибке обновить (тот же язык, что у соседних xor-сообщений в `cli.rs`).
3. `--conf` с `--prompt-file` разрешён; без `--conf` — default `<process-cwd>/harness.json5`, как для `--prompt`.
4. `--prompt-file` с `--conf-new` запрещён (уже следует из xor).
5. Приоритет справочных флагов не менять: `--help` → `--help-tools` → `--version` побеждают независимо от `--prompt-file`.
6. Парсер по-прежнему без clap.

### 2. Чтение файла
1. Путь относительный — от текущей директории **процесса**, не от `settings.cwd` и не от каталога `--conf`.
2. Файл читается как UTF-8. Содержимое **не** trim-ить по краям (пробелы и перевод строки в конце файла — часть промпта), кроме опционального снятия одного trailing BOM `\u{feff}` в начале, если он есть.
3. Ошибки (английский или в стиле соседних CLI-ошибок, но текст фиксированный и проверяемый тестом):
   - файла нет / не читается: `cannot read prompt file '<path>': <io error>`
   - путь указывает на каталог: `cannot read prompt file '<path>': is a directory`
   - не UTF-8: `cannot read prompt file '<path>': invalid utf-8`
   - файл пустой (0 байт, или только BOM): `prompt file '<path>' is empty`
4. Чтение файла — в `main.rs` (или маленькой функции рядом), **после** успешного `parse_args`. `CliAction::Start` по-прежнему несёт `prompt: Option<String>`: для `--prompt-file` парсер кладёт путь в отдельное поле (например `prompt_file: Option<PathBuf>`), а `run()` читает файл и передаёт текст в `start_session` как `prompt: Some(text)`. Не тащить чтение файла в `parse_args`.
5. Дальше сессия идёт тем же путём, что `--prompt`: `AgentLoop::start(Some(text), dump_file, None)`.

### 3. Справка и README
1. `crates/harness/help.txt`:
   - в `USAGE`: `harness.exe [--conf <path>] --prompt-file <path to prompt file>`
   - в `FLAGS`: `--prompt-file <path>` — `Start a new session; the user prompt is read from the file (UTF-8).`
   - в `RULES`: `--prompt`, `--prompt-file`, `--dump` и `--conf-new` взаимоисключающие; один из них обязателен.
2. `README.md` и `README.ru.md`: одна строка в CLI-примерах и в правилах xor.

## Scope
- `crates/harness/src/cli.rs`
- `crates/harness/src/main.rs`
- `crates/harness/help.txt`
- `README.md`
- `README.ru.md`

Не менять: crate `conf`, `tools`, агентов, формат dump, схему `harness.json5`.

## Проверка корректности решения
1. `cargo test --workspace`.
2. Тесты в `cli.rs`:
   - `--prompt-file p.txt` → `Start` с `prompt_file = p.txt`, `prompt = None`, `dump = None`;
   - `--prompt-file=p.txt` — то же;
   - `--prompt x --prompt-file p` → xor-ошибка;
   - `--prompt-file p --dump d` → xor-ошибка;
   - без `--prompt`/`--prompt-file`/`--dump`/`--conf-new` → ошибка «нужен ...»;
   - `--help --prompt-file p` → `Help`;
   - `HELP` содержит `--prompt-file`.
3. Тест чтения файла (tmp-файл, без сети): валидный UTF-8 с пробелами по краям сохраняется; пустой файл и каталог — ошибки из п. 2.3.
4. Ручной шаг (не обязателен): `harness.exe --prompt-file WiKi/todo/phase-3/2-cli-prompt-file/todo.md` должен стартовать сессию (можно прервать до сети).

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
5. Если что-то непонятно, спроси!
