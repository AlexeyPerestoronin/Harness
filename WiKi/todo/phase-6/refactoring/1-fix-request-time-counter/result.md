# Результат: request time-counter не отсчитывает секунды ожидания ответа

## Итог

Исправлено: счётчик `response waiting time: X min Y sec` теперь продолжает обновляться каждую секунду не только во время ожидания HTTP-заголовков, но и во время чтения тела ответа (`response.text()`), то есть на протяжении всего ожидания ответа вендора для не-streaming моделей.

Проверки:

- `cargo build --workspace` — успех, без warnings.
- `cargo test -p agent-loop` — успех, 8 тестов прошло (добавлено 2 новых unit-теста).

## Изменения

### `crates/agent-loop/src/vendor_request.rs`

#### Было

В `VendorRequest::send_json` чтение тела выполнялось после `RequestPipeline::execute` отдельным `tokio::select!` только с ожиданием `CancellationToken`:

```rust
let status = response.status().as_u16();
let read_body = async { response.text().await.map_err(|e| transport_error(&e)) };
let body = tokio::select! {
    _ = cancel.cancelled() => {
        return Err("request cancelled".into());
    }
    result = read_body => result,
};

let body = match body {
    Ok(body) => body,
    Err(e) => {
        last_err = format!("⚠️ {attempt}-attempt request failed: {e}");
        self.log_line(&last_err).await;
        continue;
    }
};
```

`RequestPipeline::execute` завершает DurAction-цикл сразу после получения заголовков, поэтому во время чтения тела `WaitingAction` не выполнялся и счётчик оставался на `0 min 0 sec`.

#### Стало

Добавлен приватный helper `VendorRequest::read_response_body`, который читает тело ответа и одновременно с интервалом `ctx.tick` выполняет тот же `WaitingAction` (тот же формат строки, тот же `OutputType::RewriteLine`, тот же `write_log = false`), а также реагирует на отмену `CancellationToken`.

В `send_json` чтение тела теперь вызывает этот helper:

```rust
let status = response.status().as_u16();
let body = match self.read_response_body(response, &ctx).await {
    Ok(body) => body,
    Err(PipelineError::Cancelled) => return Err("request cancelled".into()),
    Err(PipelineError::Transport(e)) => {
        last_err = format!("⚠️ {attempt}-attempt request failed: {e}");
        self.log_line(&last_err).await;
        continue;
    }
    Err(PipelineError::Hook(e)) => {
        last_err = format!("⚠️ {attempt}-attempt request hook failed: {e}");
        self.log_line(&last_err).await;
        continue;
    }
};
```

`ctx` теперь передаётся в `pipeline.execute` как `ctx.clone()`, чтобы тот же контекст (включая `started_at`) можно было использовать и на фазе чтения тела.

Helper:

```rust
async fn read_response_body(
    &self,
    response: Response,
    ctx: &PipelineContext,
) -> Result<String, PipelineError> {
    let read_body = async { response.text().await.map_err(|e| transport_error(&e)) };
    tokio::pin!(read_body);

    let mut ticker = tokio::time::interval(ctx.tick);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let waiting = WaitingAction::new(self.interaction.clone());

    loop {
        tokio::select! {
            _ = ctx.cancel.cancelled() => {
                return Err(PipelineError::Cancelled);
            }
            _ = ticker.tick() => {
                waiting.execute(ctx).await?;
            }
            result = &mut read_body => {
                return result.map_err(|e| PipelineError::Transport(e));
            }
        }
    }
}
```

#### Почему

- Минимально инвазивно: retry/rate-limit логика осталась только в `send_json`, классификация `429`/`503`/`transport`/`2xx`/`4xx` не тронута.
- `MAX_ATTEMPTS = 24` и формат `X min Y sec` не изменены.
- Служебная строка по-прежнему выводится только в консоль (`write_log = false`), так как helper использует `WaitingAction`.
- Поведение отмены сохранено: отмена и на фазе заголовков, и на фазе чтения тела возвращает `"request cancelled"` (в helper — `PipelineError::Cancelled`, в `send_json` — строка `"request cancelled"`).

## Тесты

Добавлены два unit-теста в `vendor_request.rs`:

1. `read_response_body_keeps_ticking_while_body_is_slow`
   - Локальный TCP-сервер отдаёт заголовки сразу, а тело — через 2.6 секунды.
   - Проверяется, что `read_response_body` выводит минимум два значения счётчика с разными секундами.

2. `read_response_body_cancels_when_token_is_cancelled`
   - Локальный TCP-сервер отдаёт заголовки сразу и задерживает тело на 30 секунд.
   - После того как индикатор начал выводиться, токен отменяется.
   - Проверяется, что `read_response_body` возвращает `PipelineError::Cancelled` (в `send_json` это маппится в `"request cancelled"`).
