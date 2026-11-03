# Production cleanup — ручные действия

## Smoke-проверка серверного режима

1. Выполнить:

   ```shell
   cargo run -p cli-harness -- --serve
   ```

2. Убедиться, что в stdout выводятся:
   - стартовый блок `harness <version> (server mode)`;
   - `conf: <path>`;
   - сгенерированный Bearer-токен (длинная base64url-строка);
   - строка `listening on https://<host>:<port>`.

3. Проверить health-эндпоинт:

   ```shell
   curl -k https://127.0.0.1:4067/api/v1/health
   ```

   Ожидается JSON с `"status":"ok"`, `"authRequired":true`,
   `"runState":"idle"` и корректным `"configState"`.

4. Остановить сервер (`Ctrl+C`).
