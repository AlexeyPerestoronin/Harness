# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на разработку: GitHub Actions CI (Windows + Linux)
Подключить GitHub Actions: сборка и тесты на Windows и Linux при каждом push в ветку `master`.

Зачем: репозиторий `AlexeyPerestoronin/Harness` кроссплатформенный (cfg windows/unix в `process.rs` / `terminal.rs`), но CI нет — регрессии на одной из ОС не ловятся автоматически.

Ожидаемый результат: workflow в `.github/workflows/`, который на `push` в `master` на `ubuntu-latest` и `windows-latest` выполняет `cargo test` и `cargo build --release` workspace и публикует два скачиваемых артефакта с release-бинарниками.

## Функциональные требования
1. Создать workflow-файл: `.github/workflows/ci.yml` (один файл, не несколько).
2. Триггер **только** `push` в ветку `master`. Не добавлять `pull_request`, `workflow_dispatch`, cron и другие ветки — этого нет в постановке.
3. Две ОС: `windows-latest` и `ubuntu-latest`. Предпочтительна матрица `strategy.matrix.os`, два job с копипастой не нужны.
4. Шаги (минимально достаточные):
   1. `actions/checkout` (актуальный v4).
   2. Установка stable Rust (`dtolnay/rust-toolchain@stable` или эквивалент). Компоненты extra (clippy/rustfmt) не требуются.
   3. `cargo test --workspace`
   4. `cargo build --release --workspace`
5. После успешного test и release-build опубликовать **два** артефакта (`actions/upload-artifact@v4`), чтобы не нужно было собирать проект из исходников:
   1. Версия `<X.Y.Z>` — три числовых компонента из корневого `Cargo.toml`, секция `[workspace.package]`, ключ `version` (сейчас `0.1.0`). Не хардкодить версию в yaml.
   2. Имена: `harness-<X.Y.Z>` для `ubuntu-latest` (файл `target/release/harness`) и `harness-<X.Y.Z>.exe` для `windows-latest` (файл `target/release/harness.exe`).
   3. Если версия не изменилась, новые бинарники **перезаписывают** предыдущие артефакты с теми же именами (без накопления копий одной версии).
   4. Кэш сверх разумного не обязателен (`Swatinem/rust-cache` допустим).
6. Секреты / API keys не использовать. Тесты должны проходить без сети к xAI/Google/Claude.
7. Не менять код приложения «чтобы CI позеленел», кроме случая, когда без правки тесты заведомо не кроссплатформенны — тогда остановиться и спросить.
8. Не добавлять в workflow установку Python/invoke: native harness — Cargo workspace.

## Scope
- `.github/workflows/ci.yml` (создать; каталог `.github/` тоже, если его нет)
- Другие файлы не трогать.

## Проверка корректности решения
1. Локально: `cargo test --workspace` и `cargo build --release --workspace` должны проходить на текущей ОС.
2. YAML: есть `on.push.branches: master` (или эквивалент), матрица/jobs с `windows-latest` и `ubuntu-latest`, шаги checkout + rust + test + build + upload двух артефактов `harness-<X.Y.Z>` / `harness-<X.Y.Z>.exe` (версия из `Cargo.toml`).
3. Прогон на GitHub после push в `master` подтвердит зелёный статус; до пуша достаточно локальной проверки и ревью yaml.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
