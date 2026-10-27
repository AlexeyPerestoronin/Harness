# Задача на исправление: error decoding response body

После решения задачи «very long thinking» (см. C:\MyLife\Harness\WiKi\todo\phase-5\00_very_long_thinking\todo.md) появилась ошибка `decoding response body` при работе с агентами `DeepSeek` (возможно и с другими тоже, не проверял).

Пример ошибки:
```txt
agent request tools:
1. read_file({"path": "WiKi/todo/phase-5/03-improve-cli-and-api-key/todo.md"})
Read 8149symbols
✅ success

## Iteration №25
Usage:
- time = 5 min 36 sec
- tokens = 60727
- USD approximated = 0.05
- USD real = doesn't obtained from response
response waiting time: 0 min 30 sec
⚠️ 1-attempt request failed: error decoding response body
response waiting time: 0 min 31 sec
⚠️ 2-attempt request failed: error decoding response body
response waiting time: 0 min 31 sec
⚠️ 3-attempt request failed: error decoding response body
response waiting time: 0 min 31 sec
⚠️ 4-attempt request failed: error decoding response body
response waiting time: 0 min 32 sec
⚠️ 5-attempt request failed: error decoding response body
response waiting time: 0 min 31 sec
⚠️ 6-attempt request failed: error decoding response body
response waiting time: 0 min 31 sec
⚠️ 7-attempt request failed: error decoding response body
```

Необходимо исправить эту ошибку!
Изменения, послужившие причиной ошибки находятся в последнем коммите.