# Ручная проверка серверного режима

Команды ниже выполняются из корня репозитория (`C:\MyLife\Harness`).

## 1. Сборка и тесты

```powershell
cargo build --workspace
cargo test -p cli-harness
```

## 2. Запуск сервера

```powershell
cargo run -p cli-harness -- --serve --port 4067
```

Ожидание:

- в stdout печатается Bearer-токен;
- при первом старте создаются `%LOCALAPPDATA%\harness\certs\cert.pem` и `key.pem` (на Linux/macOS — в data-каталоге ОС);
- сервер принимает HTTPS на `https://127.0.0.1:4067`.

Полезные варианты:

```powershell
# фиксированный токен
cargo run -p cli-harness -- --serve --port 4067 --auth-token <base64url-32-bytes>

# пользовательские TLS-файлы
cargo run -p cli-harness -- --serve --port 4067 --tls-cert cert.pem --tls-key key.pem
```

## 3. Проверка health без токена

```powershell
curl.exe -k https://127.0.0.1:4067/api/v1/health
```

Ожидание: `200` с полями `status`, `version`, `serverTime`, `authRequired`, `runState`, `configState`.

## 4. Проверка защищённых маршрутов

```powershell
$token = "<токен из stdout>"

# без токена — 401
curl.exe -k https://127.0.0.1:4067/api/v1/run/current

# с токеном — 200 { "runState": "idle" } или состояние последней сессии
curl.exe -k -H "Authorization: Bearer $token" https://127.0.0.1:4067/api/v1/run/current
```

## 5. Запуск цикла

```powershell
curl.exe -k -H "Authorization: Bearer $token" `
  -H "Content-Type: application/json" `
  -d "{\"prompt\":\"ping\"}" `
  https://127.0.0.1:4067/api/v1/run
```

Ожидание: `202` с `runUrl: "/api/v1/run/current"`.

Дальше опрос:

```powershell
curl.exe -k -H "Authorization: Bearer $token" https://127.0.0.1:4067/api/v1/run/current
```

Если сервер ждёт ввод, в ответе будет `pendingInput` с `requestId`. Ответ:

```powershell
curl.exe -k -H "Authorization: Bearer $token" `
  -H "Content-Type: application/json" `
  -d "{\"requestId\":\"<requestId>\",\"answer\":\"<ответ>\"}" `
  https://127.0.0.1:4067/api/v1/run/input
```

Остановка:

```powershell
curl.exe -k -H "Authorization: Bearer $token" `
  -H "Content-Type: application/json" `
  -d "{}" `
  https://127.0.0.1:4067/api/v1/run/stop
```

## 6. Доверие к самоподписанному сертификату

Для браузерного web-клиента один раз добавьте `%LOCALAPPDATA%\harness\certs\cert.pem` в доверенные корневые центры сертификации (или используйте собственный `--tls-cert`/`--tls-key`).
