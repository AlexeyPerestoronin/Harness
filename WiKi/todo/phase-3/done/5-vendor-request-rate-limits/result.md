# result

## Конфиг модели
- `AgentSettings.rpm/tpm/rpd: Option<u32>` → `i64` как `token_limit` (единый тип лимитов, `-1` = выкл).
- `rpm` обязателен только у Google → `rpm`/`tpm`/`rpd` обязательны у любого вендора (одна политика для текущих и будущих агентов).
- нет ключа → `please define 'rpm' in the model conf file`; `0` / не целое / `< -1` → `'rpm' must be -1 or a positive integer` (то же для tpm/rpd).
- спецветка «Google требует rpm» убрана.

## Файлы агентов
- `agents/grok-4.6.json5`, `agents/claude-sonnet-5-5.json5`: не было rpm/tpm/rpd → `"rpm": -1, "tpm": -1, "rpd": -1`.
- Gemini-файлы: числа не трогали (`5/250000/20` и `15/250000/500`).
- Копии в `WiKi/harness/agents/` нет: актуальные модели только в корневом `agents/`.

## Слой запросов
- ретраи/sleep были размазаны по агентам (Google: RPM+24, xAI: `sleep(15s)`+24, Claude: один POST) → `crates/harness/src/vendor_request.rs`.
- `reqwest-middleware` не брали: harness blocking, без tokio; свой модуль ~200 строк проще новой async-зависимости.
- клиент собирается один раз в слое (timeout 300s), builder больше не копируется в трёх файлах.
- состояние окон RPM/TPM/RPD живёт на экземпляре агента и **не** пишется в dump: после `--dump` лимиты стартуют с нуля.

## Семантика
- RPM: интервал между началами = `60/rpm` с, иначе sleep недостачи `+ 1.0` с (как старый Google).
- TPM: скользящие 60 с, учёт `input+output` после успешного ответа; один ответ > tpm не фейлит запрос.
- RPD: скользящие 24 ч (elapsed, без таймзоны вендора); считаются попытки, дошедшие до HTTP, включая 429/503.

## Ретраи (max 24)
- 429 → лог `fail 429: model limit exceeded!`, wait лимитов, retry.
- 503 → лог `fail 503: waiting {n}s...`, sleep 5..=20 с, retry.
- транспорт → `⚠️ {attempt}-attempt request failed: ...`, wait лимитов, retry.
- прочий HTTP → сразу `HTTP {status}: {body}`.
- 15-секундный sleep xAI удалён (`from_secs_f64(15` больше нет).

## Документация
- README.md / README.ru.md: Google больше не единственный с `rpm`; для любой модели `rpm`/`tpm`/`rpd` обязательны, `-1` = не учитывать.

## Проверка
- `cargo test --workspace`: 5 тестов conf + 6 тестов vendor_request (fake clock, без реального sleep и без сети).
