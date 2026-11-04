# Задача на рефакторинг: agent-loop — XaiAgent как обёртка над OpenAiCompatAgent, общий лог статистики, clippy и dump-тест

## Цель рефакторинга:
Устранить самое заметное DRY-нарушение в `agent-loop`: `XaiAgent` содержит собственную реализацию `iteration`, `handle_tool_calls`, `finish`, учёта usage и dump-структуры, почти полностью совпадающую с `OpenAiCompatAgent`. Дополнительно вынести дублирующийся блок «Sessions statistic» в общий помощник, закрыть clippy-предупреждение в `vendor_request.rs` и добавить интеграционный тест dump-формата с реальным `Settings::source` из `conf`.

## Вносимые изменения:

### Изменение №1: `XaiAgent` как обёртка над `OpenAiCompatAgent`
- Что хотим изменить и почему это лучше: `XaiAgent` дублирует большую часть
  `OpenAiCompatAgent`; реальные отличия xAI — URL, заголовки
  (`x-grok-conv-id`), поле тела (`conversation_id`), парсер
  `cached_prompt_text_tokens` и сохранение `conv_id`/`chat_id` в dump.
  Делаем xAI тонкой обёрткой, как уже сделано для `DeepSeekAgent` и
  `AlibabaAgent`, а отличия выражаем через точки расширения общего агента.
- Как есть сейчас: `crates/agent-loop/src/agents/xai.rs` содержит
  самостоятельную реализацию агента и парсера `parse_xai_response`.
- Как должно быть:
  - В `crates/agent-loop/src/agents/openai_compat.rs` добавить точки
    расширения (например, через поля конфигурации `create` или параметры
    конструктора): URL уже передаётся; дополнительно нужны extra headers,
    extra body fields, vendor-specific маппинг cached tokens из `usage`
    (`cached_prompt_text_tokens` для xAI) и vendor-specific расширение dump
    (для xAI — `conv_id`/`chat_id`).
  - `XaiAgent` в `xai.rs` становится обёрткой `{ inner: OpenAiCompatAgent }`
    (или эквивалентной композицией) с сохранением публичной сигнатуры
    `XaiAgent::new(tools, interaction, agent) -> Result<Self, String>`.
    Реализация `Agent` для `XaiAgent` делегирует `inner`.
  - Парсер xAI больше не должен быть отдельной полной копией
    `parse_chat_completion`; xAI-специфика (`cached_prompt_text_tokens`)
    подаётся как vendor-маппинг в общий парсер/логику.
  - Сохранение/восстановление xAI-сессии (`dump_session`/`reload_session`)
    должно по-прежнему переносить `conv_id` и `chat_id`, чтобы
    `--dump` резюмировал сессию с теми же conversation id; структура
    dump может измениться, но должна быть покрыта round-trip тестом
    (см. Изменение №4).
  - Публичные экспорты `agent_loop::agents::{XaiAgent, ...}` и поведение
    для вызывающего кода (`agent_loop.rs`, `cli-harness`) не меняются.

### Изменение №2: Общий `log_usage_summary` в `agents/mod.rs`
- Что хотим изменить и почему это лучше: блок «Sessions statistic»
  (`total requests`, `total tokens`, `input/output/cached`,
  `USD approximated`, `USD real`) повторяется в `xai.rs`,
  `openai_compat.rs` и `claude.rs`.
- Как должно быть: вынести общий помощник (например,
  `pub(crate) async fn log_usage_summary(interaction: &Arc<dyn Interaction>,
  usage: &UsageStats, total_tokens: i64)`) в `crates/agent-loop/src/agents/mod.rs`
  и использовать его в `OpenAiCompatAgent::finish`, `ClaudeAgent::finish` и
  (через `inner`) в xAI. Строка `total tokens` должна по-прежнему вычисляться
  как `usage.input_tokens + usage.output_tokens` (текущее поведение).
  Google-версия может остаться отдельной из-за другого формата строк.
- Формат выводимых строк не должен измениться.

### Изменение №3: `crates/agent-loop/src/vendor_request.rs` — redundant_closure
- Как есть сейчас: в `read_response_body` строка
  `return result.map_err(|e| PipelineError::Transport(e));`.
- Как должно быть: `return result.map_err(PipelineError::Transport);`.
  Поведение не меняется.

### Изменение №4: Интеграционный тест dump-формата с реальным `source` из `conf`
- Добавить в `crates/agent-loop/src/dump_settings.rs` (`#[cfg(test)] mod tests`)
  тест:
  - создать временный CWD и в нём минимальный валидный `harness.json5` без
    ключа `cwd` (с блоком `agent`: `config`, `mode`, `token-limit`,
    `iteration-limit`, `USD-limit`; файл `agent.config` должен существовать —
    для `load_server_config` достаточно существующего файла);
  - загрузить конфиг через `conf::load_server_config(&conf_path, &cwd)` и
    получить `ServerConfigState::Ok(settings)`;
  - проверить, что `settings.source` не содержит ключа `cwd`;
  - вызвать `embed(&dump_path, &settings.source, &cwd)`, затем
    `extract(&dump_path)`;
  - проверить, что извлечённый `settings` не содержит `cwd`, а значение
    `"CWD"` — абсолютное и совпадает с `cwd`.
- Временные файлы/каталоги создавать в `std::env::temp_dir()` с уникальными
  именами и удалять после теста.

## Проверка корректности рефакторинга:
- `cargo test -p agent-loop` — все тесты проходят, включая новый dump-тест.
- `cargo clippy -p agent-loop --all-targets` — предупреждение
  `redundant_closure` больше не выдаётся.
- `cargo check -p agent-loop` — без ошибок.
- Публичные конструкторы/экспорты агентов и поведение `AgentLoop`
  не изменены (кроме внутренней реализации xAI).

## Scope
Минимально необходимые изменения:

- `crates/agent-loop/src/agents/xai.rs`
- `crates/agent-loop/src/agents/openai_compat.rs`
- `crates/agent-loop/src/agents/claude.rs`
- `crates/agent-loop/src/agents/mod.rs`
- `crates/agent-loop/src/vendor_request.rs`
- `crates/agent-loop/src/dump_settings.rs`

Справочные материалы для чтения:

- `WiKi/todo/phase-7/pm-consulting report 3/report from refactoring manager.md`
- `crates/agent-loop/src/agents/deepseek.rs`
- `crates/agent-loop/src/agents/alibaba.rs`
- `crates/agent-loop/src/agents/google.rs`
- `crates/agent-loop/src/dump_settings.rs`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 9-agent-loop-xai-compat-and-stats-dedup → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
