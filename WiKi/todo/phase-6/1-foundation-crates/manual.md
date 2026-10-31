# Действия со стороны пользователя

Ниже — команды для проверки решения. Они уже были выполнены и завершились успешно.

```shell
cargo build --workspace
cargo test -p logger -p interactions -p auth -p harness-api
cargo test -p harness
cargo run -p harness -- --help
```

Дополнительно:
- При первом добавлении новых зависимостей Cargo обновил `Cargo.lock` (добавлены `axum`, `async-trait` и их транзитивные зависимости).
- Warnings `use of deprecated method logger::Logger::log_line` при сборке `tools` и `harness` ожидаемы: метод оставлен для компиляции старого кода и будет удалён вместе со старым бинарным крейтом.
