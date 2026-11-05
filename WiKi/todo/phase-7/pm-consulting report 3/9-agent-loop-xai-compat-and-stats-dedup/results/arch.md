# Архитектура: XaiAgent как обёртка

## До

```mermaid
classDiagram
    class XaiAgent {
        +iteration()
        +handle_tool_calls()
        +finish()
        +dump_session()
        +reload_session()
        -parse_xai_response()
    }
    class OpenAiCompatAgent {
        +iteration()
        +handle_tool_calls()
        +finish()
        +dump_session()
        +reload_session()
    }
```

`XaiAgent` дублировал реализацию `OpenAiCompatAgent`, включая парсер и учёт usage.

## После

```mermaid
classDiagram
    class XaiAgent {
        -inner: OpenAiCompatAgent
        +new()
        +delegate Agent methods
    }
    class OpenAiCompatAgent {
        +create_with(OpenAiCompatConfig)
        +iteration()
        +finish()
        +dump_session()
        +reload_session()
    }
    class OpenAiCompatConfig {
        +url
        +vendor_label
        +cached_tokens
        +vendor_session
        +vendor_headers
        +vendor_body_fields
    }
    XaiAgent --> OpenAiCompatAgent : delegates
    XaiAgent --> OpenAiCompatConfig : xAI specifics
```

`XaiAgent` стала тонкой обёрткой; xAI-отличия передаются в `OpenAiCompatAgent`
через `OpenAiCompatConfig`. Парсер xAI удалён, `cached_prompt_text_tokens`
обрабатывается vendor-маппингом в общем парсере.

## Почему

- Устранено главное DRY-нарушение в `agent-loop`.
- xAI-специфика изолирована в данных конфигурации, а не в копии протокола.
- Публичное API агентов не изменилось.
