# Результат

`keys::AGENT_SETTING = "agent-setting"` + `keys::PATHS = "paths"` → `keys::AGENT = "agent"`, `PATHS` удалён, `LOG_FOLDER` читается из `agent` (один объект настроек цикла; отдельный `paths` ради папки логов не нужен).

`paths.log-folder` / `agent-setting` в `load` → `agent.log-folder` (нет ключа → `.log`, пустая строка → ошибка); наличие `paths` или `agent-setting` → ошибка загрузки, не игнор (старые файлы нельзя принять молча).

`settings-template.json5`: объект `paths` + ключ `agent-setting` → `agent` с `"log-folder": "${vars:log-folder}"` внутри (`--conf-new` сразу в новой схеме).

`help.txt` / `README.md` / `README.ru.md`: `agent-setting.config` / `paths.log-folder` / `agent-setting` → `agent.config` / `agent.log-folder` / `agent` (документация совпадает с ключами).

тестов схемы не было → тесты `log-folder` из `agent`, дефолт `.log`, ошибки `paths` и `agent-setting`, шаблон после `vars`, `config` = файл и каталог (`cargo test --workspace` зелёный).
