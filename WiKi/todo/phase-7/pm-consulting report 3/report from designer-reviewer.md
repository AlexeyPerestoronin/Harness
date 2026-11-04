# Report from Designer Reviewer — Phase 7, Planning, Consulting 3

## Проверенные материалы
- `WiKi/todo/phase-7/po-request.md`
- `WiKi/todo/phase-7/pm-consulting report 1/report from designer-reviewer.md`
- `WiKi/todo/phase-7/pm-consulting report 2/report from designer-reviewer.md`
- `WiKi/todo/phase-7/pm-consulting report 3/base.md`
- `WiKi/todo/phase-7/pm-consulting report 3/arch.md`
- `WiKi/todo/phase-7/pm-consulting report 3/api.md`
- `WiKi/todo/phase-7/pm-consulting report 3/tech.md`
- `WiKi/todo/phase-7/pm-consulting report 3/ui.md`

Дополнительно сверено с фактическим кодом: `crates/conf` (`load.rs`, `lib.rs`,
`str/keys.rs`, `fs_scope.rs`, `types.rs`, `template.rs`, `vars.rs`),
`crates/cli-harness` (`cli.rs`, `main.rs`, `session.rs`, `server/mod.rs`,
`server/config.rs`, `server/state.rs`, `server/run.rs`), `crates/tools`
(`lib.rs`, `process.rs`, `terminal.rs`, `user.rs`, `fs.rs`, `fs/access.rs`),
`crates/agent-loop` (`agent_loop.rs`, `dump_settings.rs`),
`.github/workflows/ci.yml`, `Cargo.toml`, `harness.json5`,
`crates/conf/settings-template.json5`.

---

## 0. Статус закрытия замечаний review-2

Все критические и желательные замечания предыдущего ревью в consulting 3
закрыты:

1. **`tools.git.executable`** — явно объявлен осознанным исключением из
   правила «все пути внутри CWD»: внешняя команда/исполняемый файл,
   передаётся в `Command::new` как есть, `"git"` ищется в `PATH`
   (`base.md` решение №1, `arch.md` 3.4, `tech.md` 2.5).
2. **Число call-site'ов `load_server_config`** — исправлено на 6 и совпадает
   с кодом: `get_config`, `config_default`, `read_existing_parsed`,
   `validate_config_text`, `reload_server_config`, `refresh_server_config`.
3. **`conf/src/str/keys.rs`** — добавлен в сводную таблицу изменений.
4. **Публичный `conf::load_model`** — удаляется; в workspace вызовов
   действительно нет (проверено поиском: только объявление и re-export).
5. **Release-валидация имён артефактов** — заменена строгим regex
   `^harness-[0-9]+\.[0-9]+\.[0-9]+(-linux|-macos|\.exe|-windows\.exe)$`.
6. **Совместимость со старыми dump** — явно не требуется; `conf_dir_of`
   удаляется; dump получает верхнеуровневый `"CWD"`.
7. **`Sandbox::from_fs`** — становится fallible; инвариант «`conf`
   валидирует паттерны до цикла» зафиксирован.
8. **Glob-метасимволы и Windows case-insensitivity** — определены строго;
   единственный wildcard `*`, `?`/`[`/`]` — литералы, на Windows
   lowercasing паттерна и пути.
9. **База для `--conf`/`--dump`** — зафиксирована: оба резолвятся от
   process cwd, `--conf` дополнительно проверяется на «внутри CWD».
10. **Порядок проверок в `load_server_config`** — `ensure_conf_in_cwd`
    до чтения файла; покрывается тестом.
11. **`LoadedServerConfig.path`** — всегда абсолютный нормализованный;
    `resolve_abs_path` вызывается до загрузки.

---

## 1. Что хорошо

1. **CWD first спроектирован целостно.** Единственный источник истины —
   процесс запуска (`--cwd` либо process cwd); конфиг больше не управляет
   CWD. Проверка «внутри CWD» — компонентная (`C:\work` не считается внутри
   `C:\work2`), с лексической нормализацией `.`/`..` и учётом Windows
   case-insensitivity. Это соответствует запросу PO и устраняет реальную
   неоднозначность текущей модели, где `cwd` резолвился относительно
   `conf_dir`.
2. **Осознанные исключения корректны и обоснованы.** TLS-пути, `api-key`
   в model-файлах и `tools.git.executable` выведены из правила «все пути
   внутри CWD» с разными правилами резолва. Это снимает конфликт между
   строгой изоляцией рабочих файлов и практикой хранения сертификатов/
   секретов/исполняемых файлов вне рабочей директории.
3. **Порядок проверок зафиксирован правильно.** `ensure_conf_in_cwd` до
   `fs::metadata` означает, что `--conf` вне CWD при отсутствующем файле —
   ошибка запуска, а не `Missing`. Это ровно то поведение, которое требует
   PO, и оно явно покрыто тестом.
4. **Output buffer минимально инвазивен.** Обрезка применяется только к
   строке, возвращаемой агенту; полный стриминг в консоль человека
   сохраняется. `tail_chars` не собирает все символы в `Vec<char>`, а
   ограничивает дополнительную память последними `N` символами. Валидация
   `arg_i64` отклоняет bool/object/array/дробные и значения `< -1`.
   Опциональность аргумента с default `-1` сохраняет обратную совместимость.
5. **Deprecated-dir-patterns выбран правильно.** Строгое подмножество glob
   `**/<component>` с единственным wildcard `*` покрывает все примеры PO
   (`**/log`, `**/done`, `**/note-*`) и проще для пользователя, чем сырой
   Rust regex. Единый транслятор `conf::glob::regex_source` исключает дрейф
   между валидацией в `conf` и матчингом в `tools`. Fallible-цепочка
   `Sandbox::from_fs` → `FsModule::build` → `ToolRegistry::build` →
   `AgentLoop::new` защищает от паники при ручном вызове.
6. **Dump и resume определены без двусмысленностей.** Формат с
   верхнеуровневыми `"settings"` и `"CWD"`, запрет `--cwd` с `--dump`,
   отсутствие обратной совместимости со старыми dump — осознанный breaking
   change, зафиксированный явно.
7. **REST API и UI не перегружены.** DTO не меняются, новые эндпоинты не
   вводятся, `web-harness` не трогается. Изменения в server-слое корректно
   сведены к прокидыванию CWD в 6 call-site'ов `load_server_config` и
   хранению `LoadedServerConfig.cwd`.
8. **Release-пайплайн не переизобретается.** Доработка существующего
   `ci.yml` с `softprops/action-gh-release@v2` дешевле и надёжнее отдельного
   workflow или `gh release create`. Имена upload/download синхронизированы,
   триггер ограничен `v*`, версия тега сверяется с `Cargo.toml`, контрольные
   суммы считаются, полнота ассетов проверяется.
9. **Mermaid-диаграммы корректны.** Синтаксис валиден, потоки данных
   соответствуют коду: `argv → resolve_cwd → load_server_config →
   ensure_conf_in_cwd → agent loop/server`; `run_shell → TerminalModule →
   run_observed_command`; `CI → upload-artifact → release job → GitHub
   Release`. Логических тупиков не обнаружено.

---

## 2. Что плохо

Критических блокеров не выявлено. Ниже — точечные дефекты документов,
которые нужно поправить перед передачей в Task Manager / Senior Developer.

### 2.1. Опечатка в описании `resolve_in_cwd` для `log-folder`

`arch.md`, раздел 3.2 (`crates/conf/load.rs`):

> `log_folder_from_agent` остаётся строкой, но её значение дополнительно
> проверяется через `resolve_in_cwd(cwd, log_folder)`.

Сигнатура `resolve_in_cwd(raw, cwd)` предполагает обратный порядок:
`resolve_in_cwd(log_folder, cwd)`. Сейчас формулировка при буквальном
переносе в код даст обратную семантику (логирование CWD вместо проверки
log-folder). Это очевидная опечатка, но в планировочном документе она
недопустима.

**Как исправить:** переписать предложение на
`resolve_in_cwd(log_folder, cwd)`, либо вообще заменить на «значение
log-folder проверяется на принадлежность CWD через `resolve_in_cwd`».

### 2.2. `api.md`: тест обещает `cwd` в ответе, которого нет в DTO

`api.md`, раздел «Рекомендации»:

> `GET /api/v1/config` возвращает абсолютный `path` и корректный `cwd`.

Контракт ответа зафиксирован тут же как `path`, `exists`, `text`, `valid`,
`validation_error` — поля `cwd` в `ConfigResponse` нет. Формулировка вводит
разработчика в заблуждение: он будет искать несуществующее поле DTO.

**Как исправить:** переформулировать тест, например: «`GET /api/v1/config`
валидирует конфиг с CWD сервера; в ответе `path` абсолютный, а `valid`
отражает новую CWD first валидацию».

### 2.3. Терминология: PO назвал задачу «RegExp», реализация — glob

Заголовок в `po-request.md` — «RegExp for Deprecated-Dirs», но примеры
(`**/log`, `**/done`, `**/note-*`) — glob-паттерны. Консультация осознанно
выбрала строгое подмножество glob с единственным wildcard `*`. Это
правильное KISS-решение, однако в `help-user.txt`, шаблоне и сообщениях об
ошибках нужно использовать слово «glob/паттерн», а не «regex». Иначе
пользователь попытается задать `log.*` или `note-\d+` и получит ошибку
валидации, которую не поймёт.

**Как исправить:** в `tools/help-user.txt`, `settings-template.json5` и
текстах ошибок писать «glob pattern `**/<component>`; поддерживается только
wildcard `*`; regex не поддерживается». В `po-request.md` менять ничего не
нужно, но в документации фазы расхождение терминов должно быть закрыто.

### 2.4. Шаблон может остаться невалидным, а `config_default` обещает `valid: true`

`settings-template.json5` после правок по-прежнему будет содержать
`agent.config: "${vars:agent-config}"` со значением `"grok-4.6.json5"`
(относительно CWD). Если в CWD нет такого файла, созданный шаблон не пройдёт
`load_server_config`. При этом `POST /api/v1/config/default` в ветке
`Missing` пишет шаблон и безусловно возвращает `valid: true`, не дожидаясь
результата `refresh_server_config`.

Phase-7 меняет шаблон, поэтому это уместно поправить сейчас.

**Как исправить:** либо задать в шаблоне реально существующий путь по
умолчанию (например, `harness/agents/grok-4.6.json5`), либо в
`config_default` после `refresh_server_config` возвращать фактическое
состояние валидации, а не константу `true`.

---

## 3. Что можно улучшить

1. **Явно указать в `help.txt` базу резолва `--conf`/`--dump`.**
   В `base.md` решение зафиксировано (process cwd), но пользовательская
   справка должна явно говорить: `--conf` и `--dump` с относительным путём
   резолвятся от process cwd; `--conf` обязан оказаться внутри CWD; default
   conf — `<CWD>/harness.json5`. Это снимет типичную ошибку
   `harness --cwd work --conf harness.json5`.
2. **Удалить устаревший `tools.terminal.invoke-execution-limit` из корневого
   `harness.json5`.** Сейчас он молча игнорируется (`load_terminal` не
   отклоняет неизвестные ключи). Phase-7 вводит корневую валидацию; заодно
   стоит почистить конфиг проекта. Полную `reject_unknown_keys` для
   `tools.fs`/`git`/`terminal` можно оставить вне scope, но задокументировать
   как известный долг.
3. **Двойная компиляция паттернов.** `conf::load_fs` компилирует regex для
   валидации, затем `Sandbox::from_fs` компилирует те же паттерны повторно.
   При единицах паттернов это некритично и допустимо по KISS; если захочется
   оптимальнее — хранить `Vec<Regex>` прямо в `FsSettings`. Не блокер.
4. **Дублирование проверки «внутри CWD».** `load_fs` проверяет каждый путь
   через `resolve_in_cwd`, и `fs_scope::check_conflicts` проверяет это ещё
   раз перед сравнением. Стоит зафиксировать единственную точку
   ответственности, чтобы не разъехались сообщения об ошибках.
5. **Тесты серверного режима.** Добавить к уже перечисленным:
   `--conf` вне CWD → сервер не стартует (не только консольный режим);
   `GET /api/v1/config` отдаёт абсолютный `path`; `PUT /api/v1/config` с
   `cwd` в теле → `CONFIG_INVALID` с сообщением «CWD задаётся только через
   запуск».
6. **Symlink/junction ограничение.** Решение «проверка лексическая, без
   canonicalization» зафиксировано в `base.md`; стоит продублировать одной
   строкой в `tools/help-user.txt`, чтобы пользователь fs-tools не ожидал
   строгой изоляции через symlink.

---

## 4. Итоговый вердикт

**Утвердить концепт с точечными правками документов.**

Концепция consulting 3 жизнеспособна, соответствует запросу PO, соблюдает
KISS/YAGNI и закрывает все критические замечания review-1 и review-2.
Принципиальных архитектурных ошибок, неучтённых критических рисков или
логических тупиков в Mermaid-схемах не обнаружено.

Перед передачей в Task Manager / Senior Developer Project Manager должен
внести правки по пунктам 2.1–2.4 (опечатка `resolve_in_cwd`, тест
`GET /api/v1/config`, терминология glob vs regex, валидность шаблона и
`config_default`). Это правки документов и тестов, не затрагивающие общую
архитектуру; полный повторный design review не требуется — достаточно
проверить исправленные разделы.
