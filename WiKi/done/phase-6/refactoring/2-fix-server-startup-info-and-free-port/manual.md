# Ручная проверка

## 1. Обычный запуск на свободном порту

```sh
cargo run -p cli-harness -- --serve --port 4067
```

Ожидается:

- в stdout выведены:
  - `harness <version> (server mode)`;
  - путь к `harness.json5`;
  - `cwd` (если определён в конфигурации);
  - `host` и целевой порт;
  - источник TLS (автогенерация или пользовательские `cert/key`);
  - `live-timeout` и `interaction-timeout`;
  - CORS allowlist или `CORS disabled`;
  - Bearer-токен;
- сервер поднимается на `https://127.0.0.1:4067`.

## 2. Занятый целевой порт

Занять порт, например:

```sh
python -m http.server 4067
```

Затем в другом терминале:

```sh
cargo run -p cli-harness -- --serve --port 4067
```

Ожидается:

- в stdout выводятся шаги поиска:
  - `port 4067 is already in use, trying 4068...`;
  - при необходимости следующие шаги;
  - `selected free port <port>`;
- итоговый `listening on https://127.0.0.1:<port>` с фактическим свободным портом;
- сервер доступен на фактическом порту.

## 3. Health endpoint

```sh
curl -k https://127.0.0.1:<фактический порт>/api/v1/health
```

Ожидается HTTP `200` и тело со `status: "ok"`.
