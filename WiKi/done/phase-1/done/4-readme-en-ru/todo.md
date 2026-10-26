# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: README.en.md и README.ru.md
Добавить двуязычные README в корень репозитория: `README.en.md` и `README.ru.md`.

Зачем: у native harness нет корневой документации — как собрать, запустить и сконфигурировать бинарь, непонятно без чтения исходников.

Ожидаемый результат: два файла в корне с эквивалентным содержанием (EN и RU). Отдельный `README.md` не создавать.

## Функциональные требования
1. Создать `README.en.md` (английский) и `README.ru.md` (русский) в корне репозитория (`C:/MyLife/Harness`).
2. Содержание должно описывать **текущее** состояние кода (после unify fs и появления CI), без выдуманных фич. HTTP-сервер ещё не сделан — не упоминать как существующие.
3. Оба файла — одна структура разделов, перевод друг друга, без расхождения по фактам. В каждом файле в шапке — ссылка на другую языковую версию.
4. Обязательные разделы (заголовки можно сформулировать идиоматично, но смысл такой):
   1. **Что это.** Native agent harness на Rust: CLI `harness.exe`, цикл агента, HTTP-агенты xAI (Grok) и Google (Gemini), tool registry.
   2. **Сборка.** Cargo workspace (`crates/conf`, `crates/harness`, `crates/logger`, `crates/tools`). Команды: `cargo test --workspace`, `cargo build --release --workspace`. Итог: `target/release/harness` (`harness.exe` на Windows).
   3. **CLI.** По факту `crates/harness/help.txt`:
      - `harness.exe --help`
      - `harness.exe [--conf <path>] --prompt "<user prompt>"`
      - `harness.exe [--conf <path>] --dump "<path to dump file>"`
      - `harness.exe --conf-new "<path to new conf file>"`
      - правила xor и default `--conf` = `<process-cwd>/harness.json5`
   4. **Конфиг.** `harness.json5`: `cwd`, `paths.log-folder`, `agent-setting` (`config`, `mode`, лимиты), объект `tools`. Модельный json5: `vendor`, `name`, `api-key` (строка или путь к файлу). Известные агенты: `SpaceXAI` / `Grok 4.6`; `GoogleAI` / `Gemini 3.8 Flash`, `Gemini 3.5 Flash Lite`, `Gemini 3.1 Flash Lite`. Google требует `rpm`.
   5. **Инструменты.** Актуальные модули после unify:
      - `fs` — файлы/папки, `find_str` / `find_str_by_regex`, `find_file_by_str` / `find_files_by_regex`, `list_available_*` для dirs/files/extensions; поля `available-dirs`, `available-file-extensions`, `extra-scope` (`r:`/`w:`).
      - `git` — `executable`, `execution-limit`.
      - `terminal` — `run_shell`, `run_invoke`, `list_available_shell_commands`; `cwd-list`, `available-commands`, таймауты.
      - `user` — `ask_user`, `request_*`.
      - Нет ключей `search` и `meta`. Неизвестный ключ в `tools` — ошибка загрузки.
   6. **CI.** GitHub Actions: push в `master`, Windows и Linux, `cargo test` + `cargo build --release`.
5. Не копировать API keys, не вставлять содержимое `API_KEY_*`. Можно сказать, что `api-key` в model file — сам ключ или путь к файлу.
6. Не раздувать: без architecture diagrams, без roadmap, без badge-спама. Один CI-badge допустим, не обязателен.
7. Код приложения не менять.

## Scope
- `README.en.md` (создать)
- `README.ru.md` (создать)

## Проверка корректности решения
1. Оба файла есть в корне, разделы 4.1–4.6 присутствуют в каждом.
2. Факты сверены с `crates/harness/help.txt`, `crates/conf/src/load.rs` (`agent_kind`), `crates/conf/src/template.rs` / keys (модули tools).
3. Сборка/тесты не требуются.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
5. Если что-то непонятно, спроси!