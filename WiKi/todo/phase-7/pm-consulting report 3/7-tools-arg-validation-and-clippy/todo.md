# Задача на рефакторинг: Крейт tools — закрытие clippy и строгая валидация строковых аргументов

## Цель рефакторинга:
Закрыть clippy-предупреждения в крейте `tools` (`needless_borrows_for_generic_args` в `terminal.rs`, `map_identity` в `lib.rs`) и ужесточить валидацию строковых tool-аргументов: булевы значения больше не должны молча приводиться к строке.

## Вносимые изменения:

### Изменение №1: `crates/tools/src/terminal.rs` — needless borrow
- Что хотим изменить и почему это лучше: clippy `needless_borrows_for_generic_args`
  указывает на лишнюю ссылку у аргумента `starts_with`.
- Как есть сейчас: в `validated_cwd` строка
  `any(|entry| requested.starts_with(&resolve_from(&root, entry)))`.
- Как должно быть: убрать `&` перед `resolve_from(...)`:
  `any(|entry| requested.starts_with(resolve_from(&root, entry)))`.
  Поведение не меняется.

### Изменение №2: `crates/tools/src/lib.rs` — map_identity
- Что хотим изменить и почему это лучше: clippy `map_identity` указывает на
  бесполезное `.map_err(|e| e)?`.
- Как есть сейчас: в `ToolRegistry::call` цепочка
  `tokio::task::spawn_blocking(...).await.map_err(...)? .map_err(|e| e)?`.
- Как должно быть: убрать последнее `.map_err(|e| e)?`, оставив
  `...await.map_err(...)??` (или эквивалентную форму без identity-map).
  Поведение не меняется.

### Изменение №3: `crates/tools/src/lib.rs` — `arg_str` отклоняет bool
- Что хотим изменить и почему это лучше: сейчас `arg_str` для
  `Value::Bool(b)` возвращает `Ok(b.to_string())`. Для строковых аргументов
  путей/команд это превращает ошибочный tool-call в менее понятную
  последующую ошибку файловой системы/процесса. Числа оставляем
  строками (модели иногда присылают `depth` числом).
- Как есть сейчас:
  ```
  Some(Value::Bool(b)) => Ok(b.to_string()),
  ```
- Как должно быть:
  ```
  Some(Value::Bool(_)) => Err(format!("argument '{key}' must be a string, got boolean")),
  ```
  (точная формулировка сообщения может быть уточнена, но bool обязан
  отклоняться).
- Добавить unit-тесты в `crates/tools/src/lib.rs` (`#[cfg(test)] mod tests`):
  - `arg_str` принимает строку и JSON-число;
  - `arg_str` отклоняет bool;
  - `arg_str` отклоняет объекты/массивы/null (как и сейчас) и сообщает
    `missing argument '{key}'` для отсутствующего ключа.

## Проверка корректности рефакторинга:
- `cargo test -p tools` — все тесты проходят, включая новые.
- `cargo clippy -p tools --all-targets` — предупреждения
  `needless_borrows_for_generic_args` и `map_identity` больше не выдаются.
- `cargo check -p tools` — без ошибок.

## Scope
Минимально необходимые изменения:

- `crates/tools/src/lib.rs`
- `crates/tools/src/terminal.rs`

Справочные материалы для чтения:

- `WiKi/todo/phase-7/pm-consulting report 3/report from refactoring manager.md`
- текущие исходники `crates/tools`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 7-tools-arg-validation-and-clippy → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
