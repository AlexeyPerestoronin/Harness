# DeepSeek и AliBaba (Qwen) — результат

## Docs
Сеть для документации недоступна (в shell только `cargo`). Взято из ориентиров задачи + известного официального прайса DeepSeek, который совпадает с таблицей input/output. Если актуальные docs расходятся — поправить цены/id.

Фактические значения:
- DeepSeek URL: `https://api.deepseek.com/chat/completions` (Bearer)
- DeepSeek api-model: `deepseek-chat` (V3), `deepseek-reasoner` (R1)
- Qwen URL: `https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions` (Bearer)
- Qwen api-model: `qwen-turbo`, `qwen-plus`, `qwen-max`
- Кэш включается автоматически префиксом; отдельных `cache_control` / `enable_cache` заголовков нет
- Копии в `WiKi/harness/agents/` не создавались: агенты живут в `harness/agents/`

## Было → стало (почему)

- `AgentKind` только xAI/Google/Claude → добавлены `DeepSeek { api_model }` и `Alibaba { api_model }` + пары `from_vendor_name` из таблиц (сканер каталога смотрит на известные vendor+name)
- агентов DeepSeek/Qwen не было → `harness/agents/{deepseek-v3,deepseek-r1,qwen-turbo,qwen-plus,qwen-max}.json5` в формате Gemini-lite (`token-limit: -1`, `iteration-limit: 50`, `USD-limit: -1`, обязательные `rpm`/`tpm`/`rpd`, api-key — плейсхолдер)
- HTTP только Grok/Gemini/Claude → `deepseek.rs` / `alibaba.rs` через общий `openai_compat.rs` (chat/completions + `openai_tools()`, dump = `messages` + `usage_stats`, без grok-заголовков)
- сеть шла бы мимо лимитов → все POST только через `vendor_request`; своих sleep/retry в новых файлах нет
- кэш/USD не считались бы → `cached_tokens` из `prompt_cache_hit_tokens` / `prompt_tokens_details.cached_tokens` / `input_tokens_details.cached_tokens` (нет поля → 0); `usd = uncached/1e6*input + cached/1e6*cache_hit + output/1e6*output`; `consumed_tokens = input + output - cached`
- цена кэша DeepSeek: V3 `$0.014`, R1 `$0.14` за 1M (официальный cache-hit того же прайса, что таблица 0.14/0.28 и 0.55/2.19 — не «примерно 90%»)
- цена кэша Qwen: `$0` (открытый per-SKU cache-hit не подтверждён; коэффициент не выдуман) — комментарий у формулы
- R1 reasoning: `reasoning_content` логируется как `request reasoning:`; в history кладётся message как пришёл от API
- README / README.ru: список агентов + напоминание, что `rpm`/`tpm`/`rpd` обязательны (`-1` = н/у)

## Проверка
- `cargo test -p conf -p harness`: 4 + 8 тестов ok
- conf: 5 новых пар + старые резолвятся; `DeepSeek Nope` → `unknown agent`; `load_model` с `rpm: -1` и `rpm: 1500`; scan tmp-каталога находит `DeepSeek-V3`
- парсер: cache hit USD, отсутствие cache-полей, Qwen tool_calls, R1 `reasoning_content`
- live `API_DEEP_SEEK` (`deepseek-chat`, повтор длинного префикса): HTTP 200, `input=492 output=2 cached=256 usd=0.000037184` (= 236×0.14 + 256×0.014 + 2×0.28) / 1e6
