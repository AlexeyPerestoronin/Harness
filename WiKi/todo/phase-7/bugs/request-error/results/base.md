# Bug fix: HTTP 415 on DeepSeek / Grok

## Было

`JsonPipeline::build` формировал запрос так:

```rust
let mut req = client.post(&self.url).json(payload);
for (key, value) in &self.extra_headers {
    req = req.header(key, value);
}
```

`reqwest::RequestBuilder::json(payload)` уже добавляет заголовок
`Content-Type: application/json`. Все агенты дополнительно передавали
`("Content-Type", "application/json")`, а `RequestBuilder::header` не заменяет,
а добавляет ещё одно значение. В итоге запрос уходил с двумя
`Content-Type`, и DeepSeek / xAI отвечали:

```text
HTTP 415: Expected request with `Content-Type: application/json`
```

## Стало

`JsonPipeline::build` пропускает пользовательский `Content-Type`, потому что
заголовок уже выставлен вызовом `.json(payload)`:

```rust
let mut req = client.post(&self.url).json(payload);
for (key, value) in &self.extra_headers {
    // `.json(payload)` already sets `Content-Type: application/json`.
    // Adding it again produces a duplicate header which DeepSeek and
    // xAI reject with HTTP 415.
    if key.eq_ignore_ascii_case(reqwest::header::CONTENT_TYPE.as_str()) {
        continue;
    }
    req = req.header(key, value);
}
```

Добавлены unit-тесты:

- `json_pipeline_sets_content_type_once` — даже если вызывающий код
  передал `Content-Type`, в собранном запросе он ровно один.
- `json_pipeline_keeps_other_headers` — остальные заголовки
  (`Authorization`, `x-grok-conv-id`) сохраняются.

## Почему

`Content-Type` — одиночный заголовок, дублирование делает HTTP-запрос
невалидным с точки зрения API DeepSeek и xAI. Исправление сделано в общем
пайплайне `JsonPipeline`, чтобы ни один агент не мог снова передать
дублирующий `Content-Type`.

## Проверка

```text
cargo test -p agent-loop --lib
cargo build -p cli-harness
```

Оба завершаются успешно.
