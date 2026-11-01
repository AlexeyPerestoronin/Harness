# Ручная проверка локального сервера

## 1. Запуск сервера

```powershell
cargo run -p cli-harness -- --serve
```

Сервер напечатает Bearer-токен в stdout. Далее он используется как `<token>`.

## 2. Проверка FS API

```powershell
# Содержимое каталога roles
curl.exe -k -H "Authorization: Bearer <token>" "https://localhost:4067/api/v1/fs/list?path=roles"

# Чтение JSON5-файла агента: api-key должен вернуться как "***"
curl.exe -k -H "Authorization: Bearer <token>" "https://localhost:4067/api/v1/fs/read?path=harness/agents/deepseek-v3.json5"

# Чтение harness.json5 через FS API должно вернуть 403 FORBIDDEN_PATH
curl.exe -k -H "Authorization: Bearer <token>" "https://localhost:4067/api/v1/fs/read?path=harness.json5"
```

## 3. Проверка Config API

```powershell
# Текущая конфигурация с redaction
curl.exe -k -H "Authorization: Bearer <token>" "https://localhost:4067/api/v1/config"

# Создание конфигурации по умолчанию (если файл отсутствует)
curl.exe -k -X POST -H "Authorization: Bearer <token>" "https://localhost:4067/api/v1/config/default"

# Сохранение конфигурации
curl.exe -k -X PUT -H "Authorization: Bearer <token>" -H "Content-Type: application/json" -d '{"text":"..."}' "https://localhost:4067/api/v1/config"
```

Ожидаемые сценарии — по `api.md` §4.
