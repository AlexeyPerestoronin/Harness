# Отчёт: English-only документация и README с архитектурой, технологиями и pipeline

## Изменение №1: удаление русскоязычной документации

**Было:**
- В корне репозитория существовал `README.ru.md` — полный русскоязычный дубль `README.md`.
- В начале `README.md` была ссылка `[Русский](README.ru.md)`.

**Стало:**
- Файл `README.ru.md` удалён.
- Ссылка `[Русский](README.ru.md)` удалена из `README.md`.
- `README.md` остаётся единственным пользовательским README и полностью написан на английском языке.

**Почему:**
- Project Owner требует оставить в репозитории только англоязычную документацию.
- Один источник истины (`README.md`) исключает рассинхронизацию двух языковых версий.

## Изменение №2: раздел Architecture в README.md

**Было:**
- Явный раздел `Architecture` в `README.md` отсутствовал; состав крейтов упоминался только в разделе `Build`.

**Стало:**
- Добавлен раздел `## Architecture`:
  - таблица нативных крейтов и их ответственности: `logger`, `interactions`, `auth`, `harness-api`, `conf`, `tools`, `agent-loop`, `cli-harness`;
  - `web-harness` описан как отдельный WASM-крейт, исключённый из Cargo workspace и собираемый через Trunk;
  - Mermaid-диаграмма `flowchart` с потоком: `web-harness` → HTTPS REST + polling 10s → `cli-harness` → `conf` → `agent-loop` → vendor LLM APIs;
  - зафиксировано, что клиент и сервер общаются только по HTTPS REST с polling каждые 10 секунд; без SSE/WebSocket.

**Почему:**
- Пользователь/разработчик должен сразу видеть, из каких частей состоит система и как они взаимодействуют.

## Изменение №3: раздел Technologies в README.md

**Было:**
- Технологии упоминались разрозненно по тексту (`Trunk`, `Leptos`, `axum` и т.д.), без единого перечня.

**Стало:**
- Добавлен раздел `## Technologies` со списком и кратким назначением:
  - Rust / tokio;
  - axum + axum-server + rustls;
  - rcgen;
  - async reqwest;
  - Leptos CSR;
  - Trunk;
  - gloo-net;
  - serde / serde_json;
  - JSON5.

**Почему:**
- Явно зафиксирован используемый стек и назначение каждой технологии.

## Изменение №4: раздел базовых пользовательских pipeline

**Было:**
- Сценарии работы были частично описаны в разделах `CLI`, `Server mode`, `Web client` и `web-harness integration`, но не выделены как базовые workflow.

**Стало:**
- Добавлен раздел `## Basic workflows` с тремя сценариями:
  1. **Console session**: `harness.exe --conf harness.json5 --prompt "..."` → выбор агента/ключа при необходимости → сессия → `log.md` / `dump.json`; продолжение через `--dump`.
  2. **Server + web client**: `harness.exe --serve` → Bearer-токен из stdout → открыть `harness.wasm` → подключиться → `POST /run` → polling `runUrl` каждые 10 секунд → ответ на `pendingInput` → результат.
  3. **Embedded plugin**: подключение `web-harness` через Cargo/Git и рендеринг `HarnessPanel`; опциональная фича `fs-panel` для `ConfigEditor` и `FsBrowser`.

**Почему:**
- Пользователь должен понимать основные сценарии работы без чтения разрозненных разделов.

## Ссылка на REST-API.help.md

**Было/стало:**
- Файл `REST-API.help.md` в репозитории отсутствует, поэтому ссылка на него в `README.md` не добавлялась (согласно условию «если файл уже создан»).

**Почему:**
- Не добавляем ссылку на несуществующий документ.

## Проверка

- `README.ru.md` удалён; ссылка `[Русский]` отсутствует.
- `README.md` не содержит русскоязычных фрагментов.
- В `README.md` присутствуют разделы `## Architecture`, `## Technologies`, `## Basic workflows`.
- Команды и флаги в README соответствуют `crates/cli-harness/help.txt`: `--serve`, `--host`, `--port`, `--conf`, `--cwd`, `--auth-token`, `--tls-cert`, `--tls-key`, `--prompt`, `--prompt-file`, `--dump`, `--conf-new`, `--api-key`.
- Mermaid-диаграмма использует синтаксис `flowchart LR` и корректные связи.
