# Действия с вашей стороны

Live-тесты доступности моделей **не** входят в `cargo test --workspace` (`#[ignore]`).

Запуск вручную (ключ каждого вендора спросит в консоли; settings — из `./agents/*.json5`):

```
cargo test -p harness -- --ignored --nocapture --test-threads=1
```

Один вендор:

```
cargo test -p harness claude::tests::live_availability -- --ignored --nocapture
cargo test -p harness xai::tests::live_availability -- --ignored --nocapture
cargo test -p harness google::tests::live_availability -- --ignored --nocapture
```

История вопрос-ответ: stdout и `.log/live/<YYYY-MM-DD HH-MM-SS>_<vendor>_<name>/log.md`.
