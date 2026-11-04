# Задача на рефакторинг: Включить clippy-гейт в CI

## Цель рефакторинга:
Закрепить чистоту кодовой базы: после закрытия всех clippy-предупреждений в native-крейтах добавить шаг `cargo clippy --workspace --all-targets -- -D warnings` в GitHub Actions, чтобы предупреждения не накапливались снова.

## Вносимые изменения:

### Изменение №1: `.github/workflows/ci.yml`
- В job `ci` (матрица `ubuntu-latest` / `windows-latest` / `macos-latest`),
  после шага установки Rust добавить clippy-компонент к action
  `dtolnay/rust-toolchain@stable`, если это необходимо:
  ```
  - name: Install stable Rust
    uses: dtolnay/rust-toolchain@stable
    with:
      components: clippy
  ```
  (Если action и так устанавливает clippy, явное указание компонента не
  повредит и делает шаг самодокументированным.)
- Добавить отдельный шаг после «Install stable Rust» (или после «Test»):
  ```
  - name: Clippy (deny warnings)
    run: cargo clippy --workspace --all-targets -- -D warnings
  ```
- Поскольку `crates/web-harness` исключён из workspace, команда покрывает
  все native-крейты. Дополнительные изменения в job `web` не требуются.

## Проверка корректности рефакторинга:
- Локально выполнить `cargo clippy --workspace --all-targets -- -D warnings`
  и убедиться, что команда завершается с кодом 0 (все предупреждения из
  отчёта Refactoring Manager должны быть уже закрыты задачами 6–9).
- Убедиться, что YAML workflow корректен (синтаксически) и что новый шаг
  добавлен только в job `ci`, а не в `web`/`deploy-pages`/`release`.

## Scope
Минимально необходимые изменения:

- `.github/workflows/ci.yml`

Справочные материалы для чтения:

- `.github/workflows/ci.yml`
- `Cargo.toml`
- `WiKi/todo/phase-7/pm-consulting report 3/report from refactoring manager.md`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 10-enable-clippy-in-ci → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
