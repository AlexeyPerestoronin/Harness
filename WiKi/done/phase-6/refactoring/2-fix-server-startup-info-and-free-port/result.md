# Результат: стартовая информация сервера и поиск свободного порта

## Изменения

Файл: `crates/cli-harness/src/server/mod.rs`.

### 1. Стартовая информация

**Было:**

```rust
let auth_token = match options.auth_token {
    Some(raw) => AuthToken::parse(&raw)...,
    None => AuthToken::generate(),
};
println!("{}", auth_token.as_base64url());
```

и далее сразу:

```rust
let tls_paths = tls::resolve_tls_paths(&server)?;
let app = handlers::router(state.clone());
let addr = resolve_addr(&server.host, server.port).await?;
```

В stdout выводился только Bearer-токен.

**Стало:**

Bearer-токен печатается сразу после генерации/парсинга (как и раньше), а перед запуском live-timeout задачи и bind выводится справочный блок:

```text
harness <version> (server mode)
conf: <abs path>
cwd: <abs path>               # только если определён в настройках
host: <host>
port: <port>
tls: auto-generated in <dir>  # или tls: cert=<cert> key=<key>
live-timeout: <n>m
interaction-timeout: <n>m
CORS allowlist: <origins>     # или CORS disabled
```

Блок формируется в новой функции `print_startup_info` из уже вычисленных `ServerState`, `conf::ServerConfig` и `tls::TlsPaths`.

**Почему:** запуск сервера должен сразу сообщать оператору все значимые параметры, а клиенту — Bearer-токен.

### 2. Поиск свободного порта

**Было:**

```rust
let addr = resolve_addr(&server.host, server.port).await?;
...
axum_server::bind_rustls(addr, rustls_config)
    .serve(app.into_make_service())
    .await
```

Если целевой порт занят, сервер завершался с ошибкой bind.

**Стало:**

```rust
let initial_addr = resolve_addr(&server.host, server.port).await?;
let addr = find_free_port(initial_addr.ip(), server.port).await?;
println!("listening on https://{}:{}", server.host, addr.port());
...
axum_server::bind_rustls(addr, rustls_config)
```

Новая функция `find_free_port` идёт от целевого порта до `65535` включительно и проверяет каждый порт через `tokio::net::TcpListener::bind(SocketAddr::new(ip, port)).await`, после чего сразу освобождает listener.

Статус поиска:

```text
port 4067 is already in use, trying 4068...
port 4068 is already in use, trying 4069...
selected free port 4069
```

Если ни один порт до `65535` не свободен, возвращается ошибка:

```text
no free port found from <start> to 65535
```

Итоговый URL печатается после выбора фактического порта:

```text
listening on https://<host>:<port>
```

**Почему:** это убирает падение сервера при занятом целевом порте и делает фактический адрес доступным в stdout.

### 3. Unit-тест

Добавлен тест `find_free_port_skips_occupied_port`, который занимает реальный TCP-порт на `127.0.0.1` и проверяет, что `find_free_port` выбирает следующий свободный порт.

## Проверка

- `cargo build --workspace` — успех, без warnings.
- `cargo test -p cli-harness server::tests::find_free_port_skips_occupied_port` — успех.

Ручные проверки по сценариям №1–№3 вынесены в `manual.md`.
