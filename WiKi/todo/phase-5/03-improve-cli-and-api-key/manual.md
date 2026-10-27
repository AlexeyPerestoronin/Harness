# Ручные проверки: улучшить CLI и получение API-ключа

> Выполнять из корня репозитория (`C:\MyLife\Harness`). Вместо `harness.exe` можно использовать `target\debug\harness.exe` после `cargo build`.

## 1. Относительный `cwd` из конфигурации
1. Создать подкаталог `tmp-cwd-test` с `harness.json5`, в котором `"cwd": "."`, и валидный агент-файл.
2. Запустить из корня:
   `harness.exe --conf tmp-cwd-test\harness.json5 --prompt "ping" --api-key sk-test`
3. Ожидание: сессия создаёт `.log` внутри `tmp-cwd-test`, а не в корне.

## 2. `--cwd <relative>` переопределяет `cwd` конфигурации
1. Использовать ту же конфигурацию с `"cwd": "."`.
2. Запустить:
   `harness.exe --conf tmp-cwd-test\harness.json5 --cwd tmp-cwd-test\work --prompt "ping" --api-key sk-test`
   (`tmp-cwd-test\work` должен существовать).
3. Ожидание: `.log` создаётся в `tmp-cwd-test\work`; приоритет у `--cwd`.

## 3. `--api-key` с литеральным ключом и с путём к файлу
1. Литеральный ключ:
   `harness.exe --prompt "ping" --api-key sk-literal`
   Ожидание: консольный запрос ключа не появляется.
2. Файл ключа относительно CWD:
   `harness.exe --prompt "ping" --api-key API_KEY_CLAUDE`
   Ожидание: читается содержимое `C:\MyLife\Harness\API_KEY_CLAUDE` (обрезанное), ключ не выводится в лог/dump.

## 4. Файл агента с `"api-key": ""`
1. В файле агента указать `"api-key": ""`.
2. Запустить без `--api-key`:
   `harness.exe --prompt "ping"`
3. Ожидание: появится `Enter API key for <vendor> <name>: `; при пустом вводе/EOF — ошибка `api-key is required`.
4. Запустить с `--api-key sk-test`:
   `harness.exe --prompt "ping" --api-key sk-test`
   Ожидание: консольный запрос не появляется.

## 5. `--dump` с `--cwd` и `--api-key`
1. Создать сессию с дампом: `harness.exe --prompt "ping" --api-key sk-test`.
2. Найти `.log\dump.json` в последнем лог-каталоге.
3. Возобновить с переопределениями:
   `harness.exe --dump <path-to-dump.json> --cwd <existing-dir> --api-key sk-other`
4. Ожидание: конфигурация берётся из dump, но `cwd` и ключ применяются из CLI; при отсутствии `--api-key` ключ запрашивается, если в dump он пуст.

## 6. Ошибки несовместимости
1. `harness.exe --help --cwd x` — ожидается ошибка и exit code 1.
2. `harness.exe --conf-new out.json5 --api-key k` — ожидается ошибка и exit code 1.
3. `harness.exe --prompt "ping" --cwd=` и `--api-key=` — ожидается ошибка о пустом значении.
