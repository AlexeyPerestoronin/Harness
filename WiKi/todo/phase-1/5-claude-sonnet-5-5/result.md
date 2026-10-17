# Результат: Claude Sonnet 5.5

- `AgentKind` только Xai/Google → вариант `Claude { api_model }` (нужен отдельный HTTP-агент Anthropic)
- `agent_kind` не знал Claude → `("Claude", "Claude Sonnet 5.5")` → `api_model: "claude-sonnet-5-5"`; прочий `(vendor, name)` по-прежнему `unknown agent`
- не было model file → `agents/claude-sonnet-5-5.json5` в формате grok (`vendor`/`name`/`api-key`/лимиты, без rpm)
- не было HTTP-агента → `crates/harness/src/agents/claude.rs`: Messages API (`https://api.anthropic.com/v1/messages`, `x-api-key`, `anthropic-version: 2023-06-01`), tools как `{name, description, input_schema}`, ход как у xAI (prompt → HTTP → `tool_use`/`tool_result` или стоп), USD = `0.0`, dump `{messages, usage}` (не xAI/Google)
- loop не собирал Claude → ветка `AgentKind::Claude` → `ClaudeAgent`; `vendor()`/`model()` из conf
- description harness только xAI/Google → добавлен Claude
- conf-тест Claude не было → `load_claude_sonnet_5_5` + парсер фикстуры text+tool_use+usage
- `README.en.md` / `README.ru.md` в корне нет (задача 4) → не создавал и не правил

Проверка: `cargo test --workspace` — conf 14, harness 14, tools 30.

# Результат: revision-1

- Grok USD всегда 0 → `parse_xai_response` брал только `cost_usd`/`usage.cost` как f64 → `parse_cost_usd` читает `cost_in_usd`/`cost_usd`/`cost`/`total_cost_usd`/`total_cost` в `usage` и в корне, число или numeric string (xAI отдаёт cost не в том ключе/типе, что ждал парсер)
- не было live-тестов доступности → `#[ignore]` `live_availability` у Claude/xAI/Google: ключ с stdin, вопрос «Ты доступен? Какой ты AI?», проверяем `Ok` и tokens > 0; `cargo test --workspace` их не гоняет
- Claude live проверен ключом `API_KEY_CLAUDE` → ok, ключ из кода удалён

Проверка: `cargo test --workspace` — 16 harness (3 ignored) + conf 14 + tools 30; `claude::tests::live_availability --ignored` — ok.

# Результат: revision-2

- live-тесты писали в NullLogger → DoubleLogger в `.log/live/<timestamp>_<vendor>_<name>/log.md` (нужно смотреть историю вопрос-ответ)
- синтетический `live_support::settings` (vendor/name/kind/rpm вручную) → `conf::load_model(./agents/*.json5)` + API-ключ с stdin (реальные model files, ключ не из плейсхолдера)
- не было загрузки model file без harness.json5 → `conf::load_model` (лимиты из model file; mode задаёт вызывающий)

Проверка: `cargo test --workspace` — conf 15, harness 16 (3 ignored), tools 30.
