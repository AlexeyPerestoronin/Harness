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
