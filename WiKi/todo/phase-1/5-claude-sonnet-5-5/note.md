# Действия с вашей стороны

Не требуются для приёмки задачи.

Чтобы реально запустить Claude (сеть в тестах не дергалась):

1. В `agents/claude-sonnet-5-5.json5` заменить плейсхолдер `api-key` на ключ или путь к `API_KEY_CLAUDE`.
2. В `harness.json5` указать `"config"` на этот model file.
3. `harness.exe --prompt "..."`.
