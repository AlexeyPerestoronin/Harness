# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: Вендоры DeepSeek и AliBaba (Qwen)
Подключить к harness модели DeepSeek и Qwen (AliBaba DashScope): `AgentKind`, HTTP-агенты, файлы конфигурации, учёт кэша токенов и стоимости.

Зачем: расширить набор моделей. У DeepSeek и Qwen кэш префикса/контекста дешевле обычного input — без корректного учёта `cached_tokens` сессия будет врать в USD.

Предусловие: выполнена задача 5 (`vendor_request.rs` существует, `rpm`/`tpm`/`rpd` обязательны, `-1` = не учитывать). Не менять схему `harness.json5`, skills, dump.

Ожидаемый результат: в `agents/` лежат json5 на каждую модель из таблиц; `AgentKind::from_vendor_name` их знает; цикл стартует так же, как для Grok (OpenAI-compatible chat completions + tools), лимиты запросов идут через `vendor_request`.

## Модельный ряд и лимиты
Не учитывается = `-1` в json5.

### DeepSeek (`vendor`: `DeepSeek`)
| name в json5 | Ориентир api-model | Input / Output USD за 1M | rpm | tpm | rpd |
|---|---|---|---|---|---|
| `DeepSeek-V3` | уточнить по docs (обычно `deepseek-chat`) | 0.14 / 0.28 | -1 | -1 | -1 |
| `DeepSeek-R1` | уточнить по docs (обычно `deepseek-reasoner`) | 0.55 / 2.19 | -1 | -1 | -1 |

### AliBaba / Qwen (`vendor`: `AliBaba`)
| name в json5 | Ориентир api-model | Input / Output USD за 1M | rpm | tpm | rpd |
|---|---|---|---|---|---|
| `Qwen-Turbo` | `qwen-turbo` | 0.04 / 0.08 | 1500 | 1_000_000 | -1 |
| `Qwen-Plus` | `qwen-plus` | 0.11 / 0.28 | 1500 | 1_000_000 | -1 |
| `Qwen-Max` | `qwen-max` | 1.65 / 4.40 | 60 | 60_000 | -1 |

`api-model` и base URL **сверить с официальной документацией** (DeepSeek OpenAI-compatible API, DashScope compatible-mode). Если id модели, URL или заголовки отличаются от ориентиров — взять из docs и записать фактические в `result.md`. Если docs не открываются / противоречат — **остановись и спроси**, не выдумывай.

Ориентиры URL (проверить):
- DeepSeek: `https://api.deepseek.com/chat/completions` (Bearer)
- Qwen: `https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions` (Bearer)

## Функциональные требования
### 1. Файлы агентов
1. Создать в `agents/`:
   - `deepseek-v3.json5`
   - `deepseek-r1.json5`
   - `qwen-turbo.json5`
   - `qwen-plus.json5`
   - `qwen-max.json5`
   Формат как у существующих: `vendor`, `name`, `api-key` (плейсхолдер `"api-key or path to file with api-key"`), `token-limit`, `iteration-limit`, `USD-limit`, **обязательно** `rpm`/`tpm`/`rpd` из таблицы.
2. Сессионные лимиты в файле агента — как у Gemini-lite: `token-limit: -1`, `iteration-limit: 50`, `USD-limit: -1` (реальные лимиты сессии всё равно задаёт `harness.json5`).
3. Копии в `WiKi/harness/agents/` — те же пять файлов (чтобы выбор агента из wiki-папки их видел).
4. Корневые `API_KEY_*` не создавать.

### 2. `AgentKind` и загрузка
1. В `crates/conf/src/types.rs` добавить варианты, например:
   - `AgentKind::DeepSeek { api_model: String }`
   - `AgentKind::Alibaba { api_model: String }`
2. `from_vendor_name`: пары из таблиц (`"DeepSeek"+"DeepSeek-V3"` и т.д.). Неизвестная пара по-прежнему `unknown agent: ...`.
3. Сканер каталога агентов подхватит новые файлы автоматически (он смотрит на известные пары vendor+name). Тест: tmp-каталог с одним новым файлом → кандидат находится.
4. `AgentLoop::new` — две новые ветки `match`, как для Xai/Google/Claude.

### 3. HTTP-агенты
1. Новые модули `crates/harness/src/agents/deepseek.rs` и `alibaba.rs` (имя файла — `alibaba.rs`, не `qwen.rs`).
2. Протокол — OpenAI-compatible `chat/completions` + `openai_tools()` из `agents/mod.rs`. Можно вынести общий хелпер парсинга OpenAI-ответа (рядом, не в `xai.rs`), если без этого будет копипаста с xAI; **сам `xai.rs` не рефакторить** ради этой задачи (заголовки xAI другие: `x-grok-conv-id`, `cost_in_usd_ticks`).
3. Все сетевые вызовы — только через `vendor_request` (задача 5). Своих sleep/retry/RPM в новых файлах нет.
4. Trait `Agent`: dump/reload в том же вендорном JSON, что xAI (`messages` + `usage_stats`; `conv_id`/`chat_id` не нужны, если вендор их не требует — не имитировать grok-заголовки).
5. DeepSeek-R1 — reasoning-модель: если в ответе есть `reasoning_content` (или аналог из docs), логировать как xAI (`request reasoning:`), в `messages` класть то, что API ожидает на следующем ходе (не выдумывать поля).
6. Инструменты: `tool_calls` / `role: tool` как у OpenAI. Если выбранная модель/docs запрещает tools — остановись и спроси; по умолчанию считаем, что tools работают.

### 4. Кэш и USD — обязательно правильно
Это главный содержательный риск задачи. Смотри docs, не таблицу цен «из головы».

1. **Статистика.** Поля usage, которые вендор считает cache hit, мапить в `UsageStats.cached_tokens`. Для DeepSeek обычно `prompt_cache_hit_tokens` / `prompt_tokens_details.cached_tokens` (как уже сделано для xAI). Для Qwen — то поле, которое описано в DashScope usage. Если поля нет — `cached_tokens = 0`, не падать.
2. **Стоимость.** API может не прислать USD. Считать самим:
   `usd = uncached_input / 1e6 * input_price + cached_input / 1e6 * cache_price + output / 1e6 * output_price`
   где `uncached_input = max(input_tokens - cached_tokens, 0)`.
3. **Цена кэша.** Взять из официального прайса (DeepSeek cache hit / miss, Qwen context cache). Если в открытом прайсе cache hit не выделен — **не выдумывать коэффициент**: считать cache hit по цене 0 и написать это в `result.md` + одном комментарии у формулы. Не подставлять «примерно 90%».
4. Цены input/output — из таблиц выше, как константы у соответствующего агента (не в json5).
5. `consumed_tokens` для лимита сессии: как xAI, `input + output - cached` (cached уже в input). Если docs говорят иначе — docs, и строка в `result.md`.
6. В `finish()` печать как у xAI: requests, tokens (input/output/cached), total cost.

Не включать кэш «насильно» заголовками, которых нет в docs. Если вендор кэширует префикс автоматически — достаточно корректно читать usage. Если docs требуют явную опцию (`cache_control`, `enable_cache`, …) — включить **минимально**, чтобы повторные tool-итерации попадали в cache hit; без опции «на всякий случай».

### 5. Документация
`README.md` / `README.ru.md`: список известных агентов дополнить DeepSeek и AliBaba; напомнить, что `rpm`/`tpm`/`rpd` обязательны (`-1` = н/у). CLI не меняется.

## Scope
- `crates/conf/src/types.rs`
- `crates/conf/src/lib.rs` (тест `from_vendor_name` / scan)
- `crates/harness/src/agents/deepseek.rs` (создать)
- `crates/harness/src/agents/alibaba.rs` (создать)
- `crates/harness/src/agents/mod.rs`
- `crates/harness/src/agent_loop.rs` (только `match AgentKind`)
- `agents/deepseek-v3.json5`, `agents/deepseek-r1.json5`, `agents/qwen-turbo.json5`, `agents/qwen-plus.json5`, `agents/qwen-max.json5`
- `WiKi/harness/agents/` — те же пять файлов
- `README.md`
- `README.ru.md`

Не менять: `vendor_request.rs` (только **использовать** его API), `xai.rs`/`google.rs`/`claude.rs` (кроме случая общего OpenAI-парсера, вынесенного в новый файл, без изменения поведения xAI), CLI, tools, шаблон `harness.json5`.

## Проверка корректности решения
1. Тесты `conf`: все пять пар vendor+name резолвятся; неизвестное имя — ошибка; json5 с `rpm: -1` и с `rpm: 1500` грузятся `load_model`.
2. Unit-тесты парсеров на фикстурах JSON (без HTTP):
   - DeepSeek usage с `prompt_cache_hit_tokens` → `cached_tokens` и usd (uncached * 0.14 + cached * cache_price + output * 0.28) / 1e6;
   - DeepSeek без cache-полей → cached=0, usd = input/1e6*input_price + output/1e6*output_price;
   - Qwen-Max с tool_calls → имена/id/arguments;
   - reasoning у R1 не теряется в лог-ветке (хотя бы поле извлекается).
3. `AgentLoop` компилируется с новыми ветками. Grep: в `deepseek.rs`/`alibaba.rs` нет собственного `for _ in 1..=24` и нет `sleep(15`.
4. Проверить работоспособность решения "на боевом посту" для DeepSeek при помощи тестового API-ключа (см. файл `API_DEEP_SEEK`):
   - запросы должны отправляться и возвращаться с корректным ответом (не ошибка)
   - поля кэширования учитываются правильно

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
5. Если что-то непонятно, спроси!
