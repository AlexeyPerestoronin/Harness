# limits + price в файле агента

- Плоский файл модели (`token-limit`, `iteration-limit`, `USD-limit`, `rpm`, `tpm`, `rpd` в корне) → только объекты `limits` и `price`. Старые корневые ключи — ошибка загрузки, без тихой миграции (обратной совместимости нет; иначе старый файл молча прочитался бы по-старому).
- `rpm` / `tpm` / `rpd` читались с корня файла модели → читаются из `limits` и по-прежнему пишутся в `AgentSettings.rpm` / `tpm` / `rpd` (эти имена читает `VendorRequest`).
- Цена модели жила не в файле → `price.input-price` / `output-price` / `cached-price` обязательны и кладутся в `AgentSettings.input_price` / `output_price` / `cached_price`. `-1` сохраняется как `-1`, не превращается в `0` (на этом слое «бесплатно» — отдельное значение).
- `load` / `load_with` / `load_from_value` брали сессионные лимиты из `harness.json5` → так и остаётся. `limits.tokens` и `limits.USD` валидируются, но сессию не подменяют и второй порог в цикл не добавляют.
- `load_model` брал `token-limit` / `iteration-limit` / `USD-limit` с корня файла → `token_limit = limits.tokens`, `usd_limit = limits.USD`, `iteration_limit = -1` (итераций в файле модели больше нет).
- `scan_agent_dir` смотрел только `vendor` + `name` → дополнительно заполняет `token_limit`, `rpm` / `tpm` / `rpd` и три цены. Нет `limits`/`price`, битое значение или старый корневой ключ — файл в `skipped`, соседи остаются. `api-key` и `limits.USD` по-прежнему проверяются только у выбранного файла, до сессии и до HTTP.
- 13 файлов в `harness/agents` были плоскими, `iteration-limit` дублировал сессию, цен не было → лимиты перенесены как есть, `iteration-limit` удалён, цены из таблицы задачи. `vendor` / `name` / `api-key` не менялись.
- README (en/ru), секция файла модели, описывала плоские `rpm`/`tpm`/`rpd` → описаны `limits` и `price`, `-1` в цене = бесплатно, нет блока — ошибка до цикла, `iteration-limit` только в `harness.json5`. Список агентов и секция `harness.json5` не переписывались (фразы не стали ложными). Шаблон `settings-template.json5` не менялся.
- Тестов `conf` не было → пять тестов рядом с загрузчиком: пример `load_model`, сессия не подменяется файлом модели, тексты ошибок, скан каталога, все 13 файлов. `cargo test --workspace` зелёный, `crates/harness` не менялся.

Ключи — константы в `crates/conf/src/str/keys.rs`. Лишний ключ внутри `limits` / `price` (в задаче текст не задан) → `unknown key '<name>' in 'limits'` / `in 'price'`, в стиле уже существующего `unknown key` у skills/roles.
