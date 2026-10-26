# Задача на доработку:
Необходимо доработать задачу `WiKi\todo\phase-3\6-vendors-deepseek-alibaba\todo.md` в части касающейся реализации harness для DeepSeek.

## Основная цель: добиться работоспособности решения
На текущий момент попытка использовать `deepseek-v3` приводит к ошибке:
```
C:\MyLife\Harness>harness.exe --prompt "Ты доступен?"
Available agents in C:\MyLife\Harness\harness/agents:
  1) Claude Claude Sonnet 5.5        (claude-sonnet-5-5.json5)
  2) DeepSeek DeepSeek-R1            (deepseek-r1.json5)
  3) DeepSeek DeepSeek-V3            (deepseek-v3.json5)
  4) GoogleAI Gemini 3.1 Flash Lite  (gemini-3.1-flash-lite.json5)
  5) GoogleAI Gemini 3.5 Flash Lite  (gemini-3.5-flash-lite.json5)
  6) GoogleAI Gemini 3.8 Flash       (gemini-3.8-flash.json5)
  7) SpaceXAI Grok 4.6               (grok-4.6.json5)
  8) AliBaba Qwen-Max                (qwen-max.json5)
  9) AliBaba Qwen-Plus               (qwen-plus.json5)
  10) AliBaba Qwen-Turbo              (qwen-turbo.json5)
Select agent [1-10]: 3

# Agent-loop session:
- agent: DeepSeek DeepSeek-V3
- agent config: C:\MyLife\Harness\harness/agents\deepseek-v3.json5
- iteration limit: 60
- tokens limit: 256000
- usd limit: 2
- start time: 2026-10-24 14:23

# Iterations:

## Iteration №1: (tokens = 0; usd = 0)
user prompt:
```
Ты доступен?
```HTTP 400: {"error":{"message":"Invalid schema for function 'list_available_file_extension': schema must be a JSON Schema of 'type: \"object\"', got 'type: null'. (request_id: ac5c445d-b17e-49c8-9636-5584c4e6e7fb)","type":"invalid_request_error","param":null,"code":"invalid_request_error"}}
```

Необходимо исправить эту ошибку и проверить работоспособность решения следующим способом:
- собери исправленное решение: `cargo build --release`
- проверь работоспособность: `C:\MyLife\Harness\target\release\harness.exe --prompt "Ты доступен?"`
- нужно выбрать модель `DeepSeek-V3`
- любой ответ является корректным, главное, чтобы не было ошибок.

# Дополнительная задача:
Добавить поддержку для моделей `deepseek-flash` и `deepseek-v4-pro`.