Есть ошибка при запуске агентского цикла (проверялось на Grok и DeepSeek):

```
    C:\MyLife\Harness>cargo run -p cli-harness -- --prompt "Ты доступен?"
        Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.32s
        Running `target\debug\cli-harness.exe --prompt "Ты доступен?"`
    Available agents in C:/MyLife/Harness/harness/agents:
    | №  | Vendor   | Model                 | Cost (Input/Output/Cached) | Limits (Tokens/RPM/TPM/RPD) |
    |----|----------|-----------------------|----------------------------|-----------------------------|
    | 1  | Claude   | Claude Sonnet 5.5     | 2.00 / 10.00 / 0.10        | 500000 / -1 / -1 / -1       |
    | 2  | DeepSeek | DeepSeek-Flash        | 0.15 / 0.60 / 0.003        | -1 / -1 / -1 / -1           |
    | 3  | DeepSeek | DeepSeek-R1           | 0.55 / 2.19 / -1           | -1 / -1 / -1 / -1           |
    | 4  | DeepSeek | DeepSeek-V3           | 0.14 / 0.28 / 0.014        | -1 / -1 / -1 / -1           |
    | 5  | DeepSeek | DeepSeek-V4-Pro       | 0.66 / 1.98 / -1           | -1 / -1 / -1 / -1           |
    | 6  | GoogleAI | Gemini 3.1 Flash Lite | 0.25 / 1.50 / 0.025        | -1 / 15 / 250000 / 500      |
    | 7  | GoogleAI | Gemini 3.5 Flash Lite | 0.30 / 2.50 / 0.03         | -1 / 15 / 250000 / 500      |
    | 8  | GoogleAI | Gemini 3.8 Flash      | 0.375 / 1.875 / 0.0375     | -1 / 5 / 250000 / 20        |
    | 9  | SpaceXAI | Grok 4.6              | 2.00 / 6.00 / 0.50         | 200000 / -1 / -1 / -1       |
    | 10 | SpaceXAI | Grok 4.7              | 2.00 / 6.00 / 0.50         | 200000 / -1 / -1 / -1       |
    | 11 | SpaceXAI | Grok Build 0.1        | 1.00 / 2.00 / 0.20         | 256000 / -1 / -1 / -1       |
    | 12 | AliBaba  | Qwen-Max              | 2.00 / 6.00 / 0.20         | -1 / 60 / 60000 / -1        |
    | 13 | AliBaba  | Qwen-Plus             | 0.40 / 1.60 / -1           | -1 / 1500 / 1000000 / -1    |
    | 14 | AliBaba  | Qwen-Turbo            | 0.05 / 0.20 / -1           | -1 / 1500 / 1000000 / -1    |

    Select agent [1-14]: 5

    # Agent-loop session:
    - agent: DeepSeek DeepSeek-V4-Pro
    - agent config: C:/MyLife/Harness/harness/agents\deepseek-v4-pro.json5
    - iteration limit: 40
    - tokens limit: 256000
    - usd limit: 2
    - start time: 2026-11-03 14:49

    # Iterations:

    ## Iteration №1
    Usage:
    - time = 0 min 0 sec
    - tokens = 0
    - USD approximated = 0.00
    - USD real = doesn't obtained from response
    user prompt:
    ```
    Ты доступен?
    response waiting time: 0 min 0 sec         HTTP 415: Expected request with `Content-Type: application/json`
    error: process didn't exit successfully: `target\debug\cli-harness.exe --prompt "Ты доступен?"` (exit code: 1)

    C:\MyLife\Harness>cargo run -p cli-harness -- --prompt "Ты доступен?"
        Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.29s
        Running `target\debug\cli-harness.exe --prompt "Ты доступен?"`
    Available agents in C:/MyLife/Harness/harness/agents:
    | №  | Vendor   | Model                 | Cost (Input/Output/Cached) | Limits (Tokens/RPM/TPM/RPD) |
    |----|----------|-----------------------|----------------------------|-----------------------------|
    | 1  | Claude   | Claude Sonnet 5.5     | 2.00 / 10.00 / 0.10        | 500000 / -1 / -1 / -1       |
    | 2  | DeepSeek | DeepSeek-Flash        | 0.15 / 0.60 / 0.003        | -1 / -1 / -1 / -1           |
    | 3  | DeepSeek | DeepSeek-R1           | 0.55 / 2.19 / -1           | -1 / -1 / -1 / -1           |
    | 4  | DeepSeek | DeepSeek-V3           | 0.14 / 0.28 / 0.014        | -1 / -1 / -1 / -1           |
    | 5  | DeepSeek | DeepSeek-V4-Pro       | 0.66 / 1.98 / -1           | -1 / -1 / -1 / -1           |
    | 6  | GoogleAI | Gemini 3.1 Flash Lite | 0.25 / 1.50 / 0.025        | -1 / 15 / 250000 / 500      |
    | 7  | GoogleAI | Gemini 3.5 Flash Lite | 0.30 / 2.50 / 0.03         | -1 / 15 / 250000 / 500      |
    | 8  | GoogleAI | Gemini 3.8 Flash      | 0.375 / 1.875 / 0.0375     | -1 / 5 / 250000 / 20        |
    | 9  | SpaceXAI | Grok 4.6              | 2.00 / 6.00 / 0.50         | 200000 / -1 / -1 / -1       |
    | 10 | SpaceXAI | Grok 4.7              | 2.00 / 6.00 / 0.50         | 200000 / -1 / -1 / -1       |
    | 11 | SpaceXAI | Grok Build 0.1        | 1.00 / 2.00 / 0.20         | 256000 / -1 / -1 / -1       |
    | 12 | AliBaba  | Qwen-Max              | 2.00 / 6.00 / 0.20         | -1 / 60 / 60000 / -1        |
    | 13 | AliBaba  | Qwen-Plus             | 0.40 / 1.60 / -1           | -1 / 1500 / 1000000 / -1    |
    | 14 | AliBaba  | Qwen-Turbo            | 0.05 / 0.20 / -1           | -1 / 1500 / 1000000 / -1    |

    Select agent [1-14]: 10

    # Agent-loop session:
    - agent: SpaceXAI Grok 4.7
    - agent config: C:/MyLife/Harness/harness/agents\grok-4.7.json5
    - iteration limit: 40
    - tokens limit: 256000
    - usd limit: 2
    - start time: 2026-11-03 14:49

    # Iterations:

    ## Iteration №1
    Usage:
    - time = 0 min 0 sec
    - tokens = 0
    - USD approximated = 0.00
    - USD real = doesn't obtained from response
    user prompt:
    ```
    Ты доступен?
    response waiting time: 0 min 0 sec         HTTP 415: Expected request with `Content-Type: application/json`
    error: process didn't exit successfully: `target\debug\cli-harness.exe --prompt "Ты доступен?"` (exit code: 1)
```

Необходимо исправить!

Готовность исправления проверяй через запрос на harness-цикл: `cargo run -p cli-harness -- --prompt "Ты доступен?"` → я апрувну и выберу агента → если агент будет доступен, значит проблема решена.