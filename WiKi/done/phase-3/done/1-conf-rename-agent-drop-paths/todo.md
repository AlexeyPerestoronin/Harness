# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: Убрать `paths`, переименовать `agent-setting` → `agent`, перенести `log-folder`
Удалить секцию `paths` из `harness.json5`, переименовать объект `agent-setting` в `agent` и перенести ключ `log-folder` внутрь `agent`.

Зачем: секция `paths` содержит единственное поле — имя папки логов. Держать ради него отдельный объект незачем; настройки цикла агента должны жить в одном месте (`agent`). Это первое изменение схемы III фазы: все последующие задачи пишутся уже на новых ключах.

Ожидаемый результат: файл вида
```json5
{
    "vars": { ... },
    "cwd": "...",
    "agent": {
        "config": "...",
        "mode": "safe",
        "token-limit": 128000,
        "iteration-limit": 50,
        "USD-limit": 0.5,
        "log-folder": ".log"
    },
    "tools": { ... }
}
```
успешно загружается; секция `paths` и ключ `agent-setting` приводят к понятной ошибке загрузки; `session_dir` по-прежнему создаёт `<cwd>/<log-folder>/<YYYY-MM-DD HH-MM>_<vendor>_<model>/`.

## Функциональные требования
### 1. Ключи json5
1. Константа `keys::AGENT_SETTING = "agent-setting"` → заменить на `keys::AGENT = "agent"`. Литерал `"agent-setting"` в коде и тестах не оставлять.
2. Константа `keys::PATHS = "paths"` удалить. Присутствие ключа `paths` в корне `harness.json5` — ошибка загрузки (не игнорировать).
3. `keys::LOG_FOLDER = "log-folder"` остаётся; читается **из объекта `agent`**, а не из `paths`.
4. Сообщение, если найден старый ключ (английский, как остальные `ConfError`):
   - есть `agent-setting`, нет `agent`: `error → 'agent-setting' was renamed to 'agent'`
   - есть `paths`: `error → 'paths' was removed; move 'log-folder' into 'agent'`
   - есть и `agent`, и `agent-setting`: та же ошибка про rename (не молчать).
5. `log-folder` внутри `agent` необязателен: если ключа нет — значение `keys::DEFAULT_LOG_FOLDER` (`".log"`), как сейчас при отсутствии `paths`. Пустая строка — ошибка (`'log-folder' must be a non-empty string`).
6. Остальные поля `agent` (`config`, `mode`, `token-limit`, `iteration-limit`, `USD-limit`) — без изменений по смыслу и обязательности.

### 2. Типы и загрузка
1. `Settings.log_folder: String` **оставить** (его читает `agent_loop::session_dir`). Значение заполняется из `agent.log-folder`. Crate `harness` из-за этого менять не нужно, кроме текстов справки.
2. `load` / `load_with`: читать объект `agent` вместо `agent-setting`; `log_folder` брать из него. Ветку `obj.get(keys::PATHS)` удалить.
3. Тесты согласованности шаблона с ключами: вместо `PATHS` + `AGENT_SETTING` проверять `AGENT` и `LOG_FOLDER` внутри шаблона. Тесты, которые собирают фикстуры с `paths` / `agent-setting`, перевести на новую схему.
4. Публичный API `conf` (`load`, `load_with`, `load_model`, типы) по сигнатурам не ломать.

### 3. Шаблон `--conf-new`
1. `crates/conf/settings-template.json5`:
   - удалить объект `paths`;
   - переименовать `agent-setting` → `agent`;
   - `"log-folder": "${vars:log-folder}"` перенести внутрь `agent` (переменная `vars.log-folder` уже есть — оставить);
   - комментарии обновить: `log-folder` относителен к `cwd`, сессии пишутся в `<cwd>/<log-folder>/...`.
2. После подстановки `vars` шаблон — валидный json5 с ключами верхнего уровня `vars`, `cwd`, `agent`, `tools` (без `paths`).

### 4. Документация
1. `crates/harness/help.txt`: в описании `--conf` заменить `agent-setting.config` на `agent.config`.
2. `README.md` и `README.ru.md`: секция конфигурации — `agent.log-folder` вместо `paths.log-folder`, объект `agent` вместо `agent-setting`.
3. Исторические `WiKi/todo/**/harness.json5` **не** переписывать (они для старых сборок).

## Scope
- `crates/conf/src/str/keys.rs`
- `crates/conf/src/load.rs`
- `crates/conf/src/lib.rs` (тесты)
- `crates/conf/src/types.rs` — только если без этого не обойтись (поле `log_folder` у `Settings` не трогать)
- `crates/conf/settings-template.json5`
- `crates/harness/help.txt`
- `README.md`
- `README.ru.md`

Не менять: агентов (`crates/harness/src/agents/*`), `agent_loop.rs`, `cli.rs`, crate `tools`, файлы в `agents/`.

## Проверка корректности решения
1. `cargo test --workspace`.
2. Тесты в `conf`:
   - фикстура с `agent` + `log-folder` загружается, `settings.log_folder` равен заданному;
   - без `log-folder` внутри `agent` → `".log"`;
   - ключ `paths` → ошибка с текстом из п. 1.4;
   - ключ `agent-setting` → ошибка с текстом из п. 1.4;
   - шаблон после `vars::substitute` парсится, содержит `"agent"` и `"log-folder"`, не содержит `"paths"` и `"agent-setting"`;
   - тест согласованности ключей зелёный;
   - существующие сценарии `config` = файл и `config` = каталог не сломаны.
3. Ручной шаг: `harness.exe --conf-new %TEMP%\p3.json5` — в файле нет `paths`, есть `agent.log-folder`.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
5. Если что-то непонятно, спроси!
