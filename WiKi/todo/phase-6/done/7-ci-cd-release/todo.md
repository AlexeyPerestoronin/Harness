# Задача на разработку: CI/CD и релизные артефакты Phase 6

Обновить GitHub Actions и документацию: сборка нативного `cli-harness` для Windows/Linux/macOS, сборка `web-harness` через Trunk и публикация на GitHub Pages, создание GitHub Release по тегу, публикация `web-harness` как Git-тега для source-level интеграции.

Зачем: фаза должна давать готовые артефакты — `harness.exe`/`harness` для трёх ОС и `harness.wasm`/`dist` для браузера, а также корректную документацию запуска сервера и клиента.

Ожидаемый результат: CI собирает и тестирует workspace, публикует релизные бинарники и Pages; README описывает серверный режим и web-клиент.

## Функциональные требования

### 1. Обновить `.github/workflows/ci.yml`

1. Матрица ОС:
   - `ubuntu-latest`;
   - `windows-latest`;
   - `macos-latest`.
2. Шаги:
   - checkout;
   - установка стабильного Rust (`dtolnay/rust-toolchain@stable`);
   - `cargo test --workspace` (web-harness исключён из workspace);
   - `cargo build --release -p cli-harness`.
3. Чтение версии из `[workspace.package].version` (как в текущем workflow).
4. Имена артефактов и пути:
   - Windows: `harness-<version>.exe`, `target/release/cli-harness.exe`;
   - Linux: `harness-<version>-linux`, `target/release/cli-harness`;
   - macOS: `harness-<version>-macos`, `target/release/cli-harness`.
5. Загрузка артефактов (`actions/upload-artifact@v4`), удаление предыдущих артефактов с тем же именем (как в текущем workflow).
6. Отдельная задача/джоба для web:
   - установка Rust + `wasm32-unknown-unknown` target + Trunk;
   - `cd crates/web-harness && trunk build --release`;
   - загрузка `crates/web-harness/dist` как артефакт Pages (например, `actions/upload-pages-artifact@v3`).
7. Деплой GitHub Pages:
   - на push в основную ветку — публиковать `dist` через `actions/deploy-pages@v4`;
   - permissions: `pages: write`, `id-token: write`.
8. GitHub Release:
   - при создании тега `v*` (или `harness-*`) — создать Release и прикрепить три нативных бинарника (`softprops/action-gh-release` или `gh release create`).
9. `crates.io` публикация в первой версии не выполняется; `web-harness` публикуется только как Git-тег (source-level плагин).

### 2. Документация

1. Обновить `README.md` и `README.ru.md`:
   - серверный режим: `harness.exe --serve`, флаги `--port`, `--host`, `--conf`, `--cwd`, `--auth-token`, `--tls-cert`, `--tls-key`, дефолтный порт `4067`;
   - печать Bearer-токена в stdout, обязательность заголовка `Authorization`;
   - web-клиент: `harness.wasm` на GitHub Pages, подключение по HTTPS, polling 10 секунд, отсутствие лога/выбора агента/ввода ключа на клиенте;
   - самоподписанный сертификат и инструкция доверия в браузере;
   - интеграция `web-harness` через Cargo/Git с фичей `fs-panel`.
2. Проверить, что `crates/cli-harness/help.txt` содержит серверные флаги и примеры (если не был обновлён ранее).

### 3. Проверка CI-конфигурации

1. Убедиться, что `cargo test --workspace` не пытается собирать `web-harness` нативно (workspace `exclude`).
2. Убедиться, что пути артефактов соответствуют имени бинарного крейта `cli-harness`.
3. Убедиться, что `trunk build --release` вызывается из каталога `crates/web-harness`.

## Проверка корректности решения

1. Локально:
   - `cargo build --release -p cli-harness` — успех;
   - `cargo test --workspace` — успех;
   - `cd crates/web-harness && trunk build --release` — успех.
2. В CI (после push):
   - все три ОС собирают и загружают артефакты;
   - Pages-артефакт создаётся и публикуется;
   - при создании тега создаётся Release с тремя бинарниками.
3. Ручная проверка README: инструкции запуска сервера и клиента выполняются по шагам.

## Scope

Минимально необходимые изменения:

- `.github/workflows/ci.yml`
- `README.md`
- `README.ru.md`
- при необходимости `crates/cli-harness/help.txt`

Справочные материалы для чтения:

- `WiKi/todo/phase-6/consulting 5/base.md`
- `WiKi/todo/phase-6/consulting 5/tech.md`
- `WiKi/todo/phase-6/consulting 5/ui.md`
- `WiKi/todo/phase-6/consulting 5/report from designer-reviewer.md`
- `.github/workflows/ci.yml`
- `Cargo.toml`, `crates/*/Cargo.toml`
- `README.md`, `README.ru.md`

# Дополнительные указания

1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
