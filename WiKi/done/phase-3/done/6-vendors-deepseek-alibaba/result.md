# DeepSeek и AliBaba (Qwen) — результат

## Docs
Сеть для документации недоступна (в shell только `cargo`). Взято из ориентиров задачи + известного официального прайса DeepSeek, который совпадает с таблицей input/output. Если актуальные docs расходятся — поправить цены/id.

Фактические значения:
- DeepSeek URL: `https://api.deepseek.com/chat/completions` (Bearer)
- DeepSeek api-model: `deepseek-chat` (V3), `deepseek-reasoner` (R1), `deepseek-flash` (Flash), `deepseek-v4-pro` (V4-Pro)
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

## Bug: DeepSeek HTTP 400 `type: null` (solve-bug-1)

- было: `openai_tools()` отдавал `parameters: {}` для no-arg tools (`list_available_file_extension` и др.) → DeepSeek 400 `schema must be a JSON Schema of 'type: "object"', got 'type: null'`
- стало: общий `object_json_schema()` (как уже было у Claude) дописывает `"type": "object"` в пустую/`null` схему; позитивный тест — схема `read_file` не меняется; негативные — `{}` и `null` становятся `{type: object}`
- почему: корневая причина в реестре (`json!({})` без type), чинить harness-конвертер, не tools crate (как Claude)

## Модели DeepSeek-Flash и DeepSeek-V4-Pro

- было: только V3 (`deepseek-chat`) и R1 (`deepseek-reasoner`)
- стало: `from_vendor_name` + json5 `harness/agents/deepseek-flash.json5` / `deepseek-v4-pro.json5`; api-model = `deepseek-flash` / `deepseek-v4-pro`; rpm/tpm/rpd = `-1` как у остальных DeepSeek
- цены Flash/V4-Pro: отдельной таблицы не было → те же input/cache/output, что V3 (`0.14` / `0.014` / `0.28`), без выдуманного коэффициента

## Проверка
- `cargo test -p conf -p harness`: 4 + 12 тестов ok
- conf: V3/R1/Flash/V4-Pro + Qwen резолвятся; `DeepSeek Nope` → `unknown agent`
- `openai_tools`: `{}` и `null` → `{type: object}`; схема `read_file` не ломается
- `cargo build --release` ok
- live DeepSeek-V3, prompt «Ты доступен?»: HTTP без 400, ответ `Да, я доступен и готов помочь! Чем могу быть полезен?`; tokens input=1378 output=16 cached=0
