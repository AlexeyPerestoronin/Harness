# Результат: аппроксимированная и реальная стоимость в логе цикла

## `crates/harness/src/agents/mod.rs`
- Было: `UsageStats.total_cost_usd` — «стоимость, когда её сообщает вендор»; `Agent` имел только `consumed_usd()`; не было общих помощников.
- Стало: `total_cost_usd` — накопленная **аппроксимация** (токены × цены модели); добавлено `real_cost_usd: Option<f64>` с `#[serde(default)]`; в трейт `Agent` добавлен `consumed_usd_real()`; добавлены общие `approximate_usd`, `real_cost_from_body`, `accumulate_real_cost`, `format_usd`, `format_real_cost` и константа `USD_REAL_UNKNOWN`.
- Почему: одно место для формулы и разбора стоимости, старые dump без `real_cost_usd` читаются как `None`, все вендоры используют один приоритет ключей.

## `crates/harness/src/agents/openai_compat.rs`
- Было: `TokenPrices` и `usd_cost` жили здесь; `OpenAiCompatAgent::create` принимал `prices` от обёртки; в `total_cost_usd` копилась стоимость по этим ценам.
- Стало: `TokenPrices` и `usd_cost` удалены; `create` принимает только `agent: &AgentSettings` и берёт `input_price` / `output_price` / `cached_price` оттуда; `total_cost_usd` копит общую `approximate_usd`, `real_cost_usd` копит `real_cost_from_body`; `finish()` выводит `- USD approximated` и `- USD real`.
- Почему: прайсы не должны дублироваться в коде вендора; реальная стоимость отделена от аппроксимации.

## `crates/harness/src/agents/deepseek.rs` / `alibaba.rs`
- Было: локальные `prices_for` с зашитыми USD/1M, обёртка передавала их в `OpenAiCompatAgent`.
- Стало: `prices_for` удалены, `OpenAiCompatAgent::create` вызывается без цен; добавлена делегация `consumed_usd_real()`.
- Почему: жёстко заданные прайсы вендоров удалены; цены приходят из `AgentSettings`.

## `crates/harness/src/agents/xai.rs`
- Было: `parse_cost_usd` возвращал `0.0` при отсутствии полей; `total_cost_usd += parsed.cost_usd` (то есть копилась реальная стоимость).
- Стало: `parse_xai_response` возвращает `real_cost: Option<RealCost>` через общий `real_cost_from_body`; `total_cost_usd` копит `approximate_usd`; `real_cost_usd` копит `accumulate_real_cost`; `parse_cost_usd` удалён; `finish()` выводит две USD-строки.
- Почему: для xAI это смена смысла `total_cost_usd`; отсутствие поля и ноль теперь различаются.

## `crates/harness/src/agents/google.rs`
- Было: USD-статистика не велась, в `finish()` не было строки стоимости.
- Стало: цены берутся из `AgentSettings`; `total_cost_usd` копит `approximate_usd`; `real_cost_usd` копит `real_cost_from_body` по `usageMetadata`; в `finish()` добавлены `- USD approximated` и `- USD real`.
- Почему: аппроксимация и реальная стоимость должны выводиться для всех вендоров; `promptTokenCount` уже включает cached, формула с вычитанием не «чинится».

## `crates/harness/src/agents/claude.rs`
- Было: `usage.input_tokens` копировался как есть; `cached_tokens` не записывался; USD не велась.
- Стало: `cached_tokens = usage.cache_read_input_tokens`; `input_tokens = usage.input_tokens + cache_read_input_tokens + cache_creation_input_tokens`; `total_cost_usd` копит `approximate_usd`, `real_cost_usd` копит `real_cost_from_body`; `finish()` выводит две USD-строки.
- Почему: cache read тарифицируется по cached-цене, cache creation остаётся во входной цене и не пропадает.

## `crates/harness/src/agent_loop.rs`
- Было: `## Iteration №{n}: (tokens = {}; usd = {})` и время только в конце цикла.
- Стало: в начале каждой итерации выводится блок `## Iteration №N` / `Usage:` с `- time =`, `- tokens =`, `- USD approximated =`, `- USD real =`; время берётся как elapsed от старта цикла; добавлена чистая функция `iteration_header` и тесты формата.
- Почему: в логе и на консоли должны быть видны оба числа, а не одно безымянное `usd`; время должно выводиться на каждой итерации.

## Тесты
- Добавлены юнит-тесты: формула `6.62` / `6.60` / `0` и отрицательная цена; разбор real без сети (`ticks`, отсутствие ключей, явный `0`, накопление per-request, замена по `total_cost`); формат заголовка итерации.
- `cargo test --workspace` — 9 тестов зелёные, живые API не вызывались.
