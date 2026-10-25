# Задача
Необходимо сформировать dev-задачи для IV фазы разработки:
    - каждая задача должна быть сформулирована в `WiKi/todo/phase-4/<number>-<task-name>/todo.md`
      - шаблон для dev-задачи смотри в `WiKi/harness/templates/development.md`
    - для каждой задаче необходимо сформировать отдельный `harness.json5` в папке с `todo.md`
      - целевой агент я выберу самостоятельно, поэтому достаточно грамотно сформулировать tools и их ограничения.

## Задачи для IV фазы:

### Improve agents settings file format:
Необходимо изменить формат файлов конфигурации для имеющихся агентов:
- как есть сейчас:
    ```json5
    {
        "vendor": "AliBaba",
        "name": "Qwen-Turbo",
        "api-key": "api-key or path to file with api-key",
        "token-limit": -1,
        "iteration-limit": 50,
        "USD-limit": -1,
        "rpm": 1500,
        "tpm": 1000000,
        "rpd": -1,
    }
    ```
- како должно слать:
    ```json5
    {
        "vendor": "AliBaba",
        "name": "Qwen-Turbo",
        "api-key": "api-key or path to file with api-key",
        "limits": {
            "tokens": -1,
            "USD": -1,
            "rpm": 1500,
            "tpm": 1000000,
            "rpd": -1,
        },
    }
    ```
    Т.е. необходимо объединить все лимиты в один общий блок.

Важный момент:
- нет необходимости поддерживать обратную совместимость → файлы должны измениться → код не должен уметь обрабатывать старый формат.
- ключ `"iterations"` необходимо удалить, т.к. он относится только к настройками конкретной сессии и должен браться из общего файла конфигурации (`harness.json5`)

### Add price settings:
Все файлы конфигурации агентов должны получить секцию `price`.
Пример:
```json5
{
    "vendor": "AliBaba",
    "name": "Qwen-Turbo",
    "api-key": "api-key or path to file with api-key",
    "limits": {
        "tokens": -1,
        "iterations": 50,
        "USD": -1,
        "rpm": 1500,
        "tpm": 1000000,
        "rpd": -1,
    },
    "price": {
        "input-price": <some_digit_value>,  // известная стоимость за 1.000.000 входных токенов для модели
        "output-price": <some_digit_value>, // известная стоимость за 1.000.000 выходных токенов для модели
        "cached-price": <some_digit_value>, // известная стоимость за 1.000.000 кэшированных токенов для модели
    }
}
```

Опорная таблица для стоимости для моделей:
| Производитель и модель              | Входные токены (Input), $ | Выходные токены (Output), $ | Кэшированные токены (Cached), $ |
| :---                                | :---:                     | :---:                       | :---:                           |
| **Claude Claude Sonnet 5.5**        | 2.00                      | 10.00                       | 0.10                            |
| **DeepSeek DeepSeek-Flash**         | 0.15                      | 0.60                        | 0.003                           |
| **DeepSeek DeepSeek-R1**            | 0.55                      | 2.19                        | -1                              |
| **DeepSeek DeepSeek-V3**            | 0.14                      | 0.28                        | 0.014                           |
| **DeepSeek DeepSeek-V4-Pro**        | 0.66                      | 1.98                        | -1                              |
| **GoogleAI Gemini 3.1 Flash Lite**  | 0.25                      | 1.50                        | 0.025                           |
| **GoogleAI Gemini 3.5 Flash Lite**  | 0.30                      | 2.50                        | 0.03                            |
| **GoogleAI Gemini 3.8 Flash**       | 0.375                     | 1.875                       | 0.0375                          |
| **SpaceXAI Grok 4.6**               | 2.00                      | 6.00                        | 0.50                            |
| **SpaceXAI Grok 4.7**               | 2.00                      | 6.00                        | 0.50                            |
| **AliBaba Qwen-Max**                | 2.00                      | 6.00                        | 0.20                            |
| **AliBaba Qwen-Plus**               | 0.40                      | 1.60                        | -1                              |
| **AliBaba Qwen-Turbo**              | 0.05                      | 0.20                        | -1                              |


Важные моменты:
- `-1` в таблице - free - бесплатно
- в процессе работы агентского цикла на основе этих данных должны производиться вычисления аппроксимированной стоимости, что должно отображаться в логах:
    Пример:
    - как есть сейчас:
        ```md
        ## Iteration №5: (tokens = 100000; usd = 10.34)
        ```
    - как должно быть:
        ```md
        ## Iteration №5
        Usage:
        - tokens = 100000  
        - USD approximated = 10.34
        - USD real = 10.45
        ```
    - аппроксимированная стоимость должна вычисляться всегда
    - реальная стоимость не вычисляется, а получается из ответов, поэтому если в ответах не присутствует данные о стоимости сессии, то писать так `- USD real = None`
- отсутствие данных по стоимости в конфигурационном файле модели должно приводить к ошибке до начала цикла


## Improve info when choosing agent
В ситуации когда перед началом цикла пользователя просят выбрать целевой агент, необходимо улучить визуальную компоненту, а именно выводить в консоль таблицу доступных агентов с некоторой информацией о них:

Как есть сейчас:
```txt
Available agents in C:\MyLife\Harness\harness/agents:
  1) Claude Claude Sonnet 5.5        (claude-sonnet-5-5.json5)
  2) DeepSeek DeepSeek-Flash         (deepseek-flash.json5)
  3) DeepSeek DeepSeek-R1            (deepseek-r1.json5)
  4) DeepSeek DeepSeek-V3            (deepseek-v3.json5)
  5) DeepSeek DeepSeek-V4-Pro        (deepseek-v4-pro.json5)
  6) GoogleAI Gemini 3.1 Flash Lite  (gemini-3.1-flash-lite.json5)
  7) GoogleAI Gemini 3.5 Flash Lite  (gemini-3.5-flash-lite.json5)
  8) GoogleAI Gemini 3.8 Flash       (gemini-3.8-flash.json5)
  9) SpaceXAI Grok 4.6               (grok-4.6.json5)
  10) SpaceXAI Grok 4.7               (grok-4.7.json5)
  11) AliBaba Qwen-Max                (qwen-max.json5)
  12) AliBaba Qwen-Plus               (qwen-plus.json5)
  13) AliBaba Qwen-Turbo              (qwen-turbo.json5)
Select agent [1-13]: 
```

Как должно быть:
```txt
Available agents in C:\MyLife\Harness\harness/agents:
| №  | Vendor   | Model                 | Cost (Input/Output/Cached) | Limits (Tokens/RPM/TMP/RPD) |
|----|----------|-----------------------|----------------------------|-----------------------------|
| 1  | Claude   | Claude Sonnet 5.5     | 2.00  / 10.00 / 0.10       | ...take from settings...    |
| 2  | DeepSeek | DeepSeek-Flash        | 0.15  / 0.60  / 0.003      | ...take from settings...    |
| 3  | DeepSeek | DeepSeek-R1           | 0.55  / 2.19  / -1         | ...take from settings...    |
| 4  | DeepSeek | DeepSeek-V3           | 0.14  / 0.28  / 0.014      | ...take from settings...    |
| 5  | DeepSeek | DeepSeek-V4-Pro       | 0.66  / 1.98  / -1         | ...take from settings...    |
| 6  | GoogleAI | Gemini 3.1 Flash Lite | 0.25  / 1.50  / 0.025      | ...take from settings...    |
| 7  | GoogleAI | Gemini 3.5 Flash Lite | 0.30  / 2.50  / 0.03       | ...take from settings...    |
| 8  | GoogleAI | Gemini 3.8 Flash      | 0.375 / 1.875 / 0.0375     | ...take from settings...    |
| 9  | SpaceXAI | Grok 4.6              | 2.00  / 6.00  / 0.50       | ...take from settings...    |
| 10  | SpaceXAI | Grok 4.7              | 2.00  / 6.00  / 0.50       | ...take from settings...    |
| 11 | AliBaba  | Qwen-Max              | 2.00  / 6.00  / 0.20       | ...take from settings...    |
| 12 | AliBaba  | Qwen-Plus             | 0.40  / 1.60  / -1         | ...take from settings...    |
| 13 | AliBaba  | Qwen-Turbo            | 0.05  / 0.20  / -1         | ...take from settings...    |

Select agent [1-13]: 
```

Важные моменты:
- таблица должна выводиться в консоль в отформатированном виде

# Особые указания
Код менять нельзя, только формулировка задач.
Задачи можно дробить, объединять и менять в последовательности реализации если это представляется целесообразным.
Главное, чтобы отдельные задачи не пересекались по scope и логике.