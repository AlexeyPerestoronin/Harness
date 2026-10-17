# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: Поддержка Claude Sonnet 5.5
Добавить вендора Claude и модель `claude-sonnet-5-5` рядом с уже существующими xAI и Google агентами.

Зачем: в репозитории уже есть файл ключа `API_KEY_CLAUDE`, но `agent_kind` знает только SpaceXAI/GoogleAI — запустить Claude нельзя.

Ожидаемый результат: model json5 с `vendor: "Claude"` и `name: "Claude Sonnet 5.5"` загружается, loop создаёт HTTP-агента к Anthropic Messages API с `model: "claude-sonnet-5-5"`, инструменты и dump/reload работают как у остальных агентов.

## Функциональные требования
### 1. Идентичность модели
1. Пара `(vendor, name)` в model file:
   - `vendor` = `Claude`
   - `name` = `Claude Sonnet 5.5`
2. API model id, уходящий в HTTP: `claude-sonnet-5-5`.
3. Любая другая пара по-прежнему `unknown agent: {vendor} {name}`.
4. Файл модели: `agents/claude-sonnet-5-5.json5` в том же формате, что `agents/grok-4.6.json5`:
   - `vendor`, `name`, `api-key`, `token-limit`, `iteration-limit`, `USD-limit`
   - `api-key`: плейсхолдер как у других (`api-key or path to file with api-key`) либо `API_KEY_CLAUDE`
   - `rpm`/`tpm`/`rpd` для Claude **не** обязательны (это Google-only).

### 2. Conf
1. `AgentKind` дополнить вариантом `Claude { api_model: String }` в `crates/conf/src/types.rs`.
2. `agent_kind` в `crates/conf/src/load.rs` мапит `("Claude", "Claude Sonnet 5.5")` → `AgentKind::Claude { api_model: "claude-sonnet-5-5" }`.
3. Резолв `api-key` не менять (строка или путь к файлу).
4. Тест: неизвестный агент по-прежнему ошибка; новая пара загружается (можно без реального ключа — пустая загрузка с literal key в tmp, как существующие conf-тесты).

### 3. HTTP-агент
1. Новый модуль `crates/harness/src/agents/claude.rs`, экспорт в `agents/mod.rs`.
2. Реализует тот же trait `Agent`, что `XaiAgent` / `GoogleAgent`.
3. Протокол: Anthropic **Messages API**, blocking `reqwest` (async runtime не вводить).
   - URL: `https://api.anthropic.com/v1/messages`
   - Заголовки: `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`
   - Body: `model` = `claude-sonnet-5-5`, `max_tokens` (положительное число; если session `token-limit` > 0, взять его, иначе разумный потолок например 8192), `messages`, `tools`
4. Инструменты в формате Anthropic:
   ```
   { "name", "description", "input_schema": <json schema parameters> }
   ```
   Не OpenAI `function`/`parameters` и не Gemini `functionDeclarations`.
5. Ход итерации как у xAI: если есть `prompt` — user message; HTTP; если в ответе `tool_use` — выполнить через `invoke_tool`, добавить user message с `tool_result`, вернуть `iteration() = false`; если tool_use нет — `true` (сессия завершена).
6. Парсинг ответа: `content` blocks `text` и `tool_use` (`id`, `name`, `input`). Usage: `input_tokens` / `output_tokens` из `usage`. USD, если API не отдаёт — `0.0` (как Google).
7. Логирование в том же стиле, что xAI: user prompt / agent content / tool list / success|fail.
8. `dump_session` / `reload_session`: JSON со своими полями (messages + usage). Не пытаться читать dump xAI/Google.
9. Таймаут HTTP клиента — как у других (300s).
10. Unit-тесты на парсинг фикстуры (text + tool_use + usage), без сети.

### 4. Loop / пакет
1. `AgentLoop::new`: ветка `AgentKind::Claude` → `ClaudeAgent::new(...)`.
2. `vendor()` возвращает `"Claude"`, `model()` — display name `"Claude Sonnet 5.5"` (поле `name` из conf).
3. Обновить `crates/harness/Cargo.toml` description (сейчас только xAI и Google).
4. Комментарий `crates/harness/src/agents/mod.rs` («xAI и Google») расширить.

## Scope
- `crates/conf/src/types.rs`
- `crates/conf/src/load.rs`
- `crates/conf/src/lib.rs` (тесты)
- `crates/harness/src/agents/mod.rs`
- `crates/harness/src/agents/claude.rs` (создать)
- `crates/harness/src/agent_loop.rs`
- `crates/harness/Cargo.toml`
- `agents/claude-sonnet-5-5.json5` (создать)
- `README.en.md`
- `README.ru.md`

Не менять xAI/Google агенты, кроме необходимости скомпилировать общий match.

## Проверка корректности решения
1. `cargo test --workspace`
2. Conf-тест: `(Claude, Claude Sonnet 5.5)` загружается; мусорный vendor — ошибка.
3. Тест парсера Claude response (фикстура).
4. Ручной вызов API не обязателен. Если ключ `API_KEY_CLAUDE` недоступен — не ходить в сеть.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
