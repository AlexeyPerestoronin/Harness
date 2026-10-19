# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: `help-user.txt` и CLI-команда `--help-tools`
Добавить файл `crates/tools/help-user.txt` — справку **для человека** по всем доступным tools harness и параметрам их настройки в конфигурации, и CLI-флаг `--help-tools`, который печатает эту справку.

Зачем: сейчас `--help` описывает только флаги командной строки. Как включить и ограничить инструменты (`fs`, `git`, `terminal`, `user`, `help`), какие параметры обязательны и что они значат, пользователь узнаёт только из шаблона или кода.

Предусловия: выполнены задачи 2–4 и 7 (нужны `available-files`, `deprecated-*`, модуль `help`, переменные `vars`).

Ожидаемый результат: `harness.exe --help-tools` печатает текст `help-user.txt` и завершается с кодом 0, не требуя конфигурации, ключей и сети. Текст вшит в бинарник при сборке так же, как `crates/harness/help.txt` для `--help`.

## Функциональные требования
### 1. Файл `crates/tools/help-user.txt`
1. Язык — английский (как `help.txt`). Простой текст, ширина строк до ~100 символов.
2. Структура:
   - вступление: как включаются модули (`tools.<module>` в конфигурации; нет ключа → модуль выключен; неизвестный ключ → ошибка запуска);
   - раздел на каждый модуль: `fs`, `git`, `terminal`, `user`, `help`:
     - назначение модуля;
     - **параметры конфигурации**: имя, тип, обязательность, значение по умолчанию (если есть), смысл, пример JSON5;
     - список инструментов модуля с описанием в одну строку;
   - раздел «Variables»: коротко про секцию `vars` и синтаксис `${vars:<name>}` (где применимо: любой json5-файл конфигурации — файл цикла и файл агента), 1–2 примера;
   - раздел «Access model»: префиксы `r:` / `w:` (`w` включает `r`), сопоставление директорий по префиксу абсолютного пути, `available-files` — точные файлы, `available-file-extensions` (пусто = любые), `deprecated-dirs` / `deprecated-files` (исключаются из всех операций, пути без префиксов, пересечение с `available-*` при загрузке — ошибка; допустимо вкладывать deprecated внутрь available), относительные пути считаются от текущей директории процесса.
3. Все имена параметров и инструментов **сверять с кодом**: ключи — `crates/conf/src/str/keys.rs` и `load.rs`, инструменты — `descriptors()` модулей. Не придумывать.
4. Для `terminal`: описать `cwd-list`, `available-commands`, `command-execution-limit`, `invoke-execution-limit` и отличие `run_shell` (белый список) от `run_invoke` и от `request_command_shell_execution`; для `git`: `executable` (обязателен, без значения по умолчанию), `execution-limit`.

### 2. Вшивание в бинарник
1. В `crates/tools/src/lib.rs`: `pub const HELP_USER: &str = include_str!("../help-user.txt");` (с докстрингом).
2. `crates/harness` получает текст через `tools::HELP_USER` (зависимость на `tools` уже есть).

### 3. CLI `--help-tools`
1. `crates/harness/src/cli.rs`: новый вариант `CliAction::HelpTools`; флаг `--help-tools` (без значения). Тот же приоритет, что у `--help`: срабатывает независимо от остальных флагов (`--help-tools --prompt x` → `HelpTools`, `--conf` и т.д. игнорируются). Если указаны **и** `--help`, и `--help-tools`, побеждает `--help`.
2. `crates/harness/src/main.rs`: ветка `CliAction::HelpTools` печатает `tools::HELP_USER` (с гарантированным завершающим переводом строки, как для `HELP`) и возвращает успех.
3. `crates/harness/help.txt`: добавить в `USAGE` строку `harness.exe --help-tools`, в `FLAGS` — описание: `Print the description of all tools and their configuration parameters and exit with code 0.`
4. Эта задача могла быть выполнена после изменений `cli.rs`, `main.rs`, `help.txt` в других задачах — перечитай файлы перед правкой.

## Scope
- `crates/tools/help-user.txt` (создать)
- `crates/tools/src/lib.rs` (только константа `HELP_USER`)
- `crates/harness/src/cli.rs`
- `crates/harness/src/main.rs`
- `crates/harness/help.txt`

Остальные файлы — только чтение для сверки фактов.

## Проверка корректности решения
1. `cargo test --workspace`.
2. Тесты в `cli.rs`: `--help-tools` → `HelpTools`; `--help-tools --prompt x` → `HelpTools`; `--help --help-tools` → `Help`; `HELP` содержит `--help-tools`.
3. Тесты в `tools`: `HELP_USER` непустой и содержит имена всех модулей и **каждого** ключа конфигурации модулей (`available-dirs`, `available-file-extensions`, `available-files`, `deprecated-dirs`, `deprecated-files`, `executable`, `execution-limit`, `cwd-list`, `available-commands`, `command-execution-limit`, `invoke-execution-limit`) и каждого имени инструмента из `descriptors()` всех модулей (чтобы справка не расходилась с кодом).
4. Ручной шаг: `target\debug\harness.exe --help-tools` — печатает справку, код возврата 0.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
5. Если что-то непонятно, спроси!
