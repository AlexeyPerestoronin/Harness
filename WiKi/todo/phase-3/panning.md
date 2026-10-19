1. Для всех провайдоров добавить RPM с циклом повторов
Необходимо полностью избавить от ошибки:
```C:\MyLife\Harness\crates\harness\src\agents\xai.rs
xAI request failed: error sending request for url (https://api.x.ai/v1/chat/completions)
```

2. Настроить harness для DeepSeek
3. Настроить harness для Qwen


## Особые запросы:
1. Максимально уменьшить объём кодовой базы + произвести её рефакторинг + уменьшить количество unit-тестов (возможно, вынести их в отдельное место).