# Задача на рефакторинг: cli-harness — clippy, контракт AgentLoop::new, дедупликация config_default и серверные тесты

## Цель рефакторинга:
Закрыть clippy-предупреждения в `cli-harness` и `interactions`, убрать неиспользуемый параметр `_log_folder` из `AgentLoop::new`, устранить дублирование веток в `config_default` и добавить unit-тесты для наиболее регрессионно-опасных мест серверного режима (валидация конфига без `cwd`, отклонение `cwd` в конфиге, создание default-конфига).

## Вносимые изменения:

### Изменение №1: `crates/cli-harness/src/cli.rs` — collapsible_if
- Что хотим изменить и почему это лучше: clippy `collapsible_if` указывает на
  вложенные `if`, которые можно объединить без изменения смысла.
- Как есть сейчас:
  ```
  if cwd.is_some() || api_key.is_some() {
      if help || help_tools || version || conf_new.is_some() {
          return Err(...);
      }
  }
  ```
- Как должно быть: один `if` с объединённым условием:
  ```
  if (cwd.is_some() || api_key.is_some())
      && (help || help_tools || version || conf_new.is_some())
  {
      return Err(...);
  }
  ```
  Текст ошибки и поведение сохраняются.

### Изменение №2: `crates/cli-harness/src/server/mod.rs` — unnecessary_lazy_evaluations
- Как есть сейчас:
  ```
  let tls_dir = tls_paths
      .cert
      .parent()
      .unwrap_or_else(|| tls_paths.cert.as_path());
  ```
- Как должно быть:
  ```
  let tls_dir = tls_paths.cert.parent().unwrap_or(tls_paths.cert.as_path());
  ```
  Поведение не меняется.

### Изменение №3: `crates/interactions/src/cli.rs` — manual_pattern_char_comparison
- Как есть сейчас: `answer.trim_end_matches(|c| c == '\r' || c == '\n')`.
- Как должно быть: `answer.trim_end_matches(['\r', '\n'])`.
  Поведение не меняется.

### Изменение №4: `AgentLoop::new` без неиспользуемого `_log_folder`
- В `crates/agent-loop/src/agent_loop.rs` убрать параметр `_log_folder: String`
  из сигнатуры `AgentLoop::new` (он не используется внутри). Обновить
  doc-комментарий. Если атрибут `#[allow(clippy::too_many_arguments)]`
  перестанет быть нужен (останется 6 аргументов), убрать и его.
- Обновить call-site'ы:
  - `crates/cli-harness/src/main.rs` — в `start_new_session` и
    `start_resume_session` убрать передаваемый аргумент
    (`settings.agent_session.log_folder.clone()` и
    `settings.log_folder.clone()` соответственно);
  - `crates/cli-harness/src/server/run.rs` — убрать аргумент
    `settings.agent_session.log_folder.clone()`.
- Проверить, что других вызовов `AgentLoop::new` в workspace нет.

### Изменение №5: `crates/cli-harness/src/server/config.rs` — дедупликация `config_default`
- Что хотим изменить и почему это лучше: в `config_default` блок
  `if !path.exists()` и ветка `Ok(ServerConfigState::Missing)` делают одно и
  то же (записывают шаблон, вызывают `refresh_server_config`, возвращают
  `ConfigDefaultResponse`), но тело продублировано.
- Как должно быть: вынести общий код в отдельную функцию (например,
  `async fn create_default_config_response(state, path, cwd) ->
  Result<Json<ConfigDefaultResponse>, ApiError>`), которая:
  - вызывает `write_config_file(&path, &conf::template_text())?`;
  - вызывает `refresh_server_config(state, path, cwd).await`;
  - читает `state.config.read().await.config_state`;
  - возвращает `ConfigDefaultResponse { path, exists: true,
    text: conf::template_text(), valid }`.
- Использовать эту функцию в обеих ветках (`!path.exists()` и
  `ServerConfigState::Missing`), сохранив текущее наблюдаемое поведение.

### Изменение №6: Серверные unit-тесты
- Добавить `#[cfg(test)] mod tests` в `crates/cli-harness/src/server/config.rs`.
  Минимальный набор тестов:
  - `validate_config_text` с валидным конфигом без `cwd` → `Ok`;
  - `validate_config_text` с корневым ключом `cwd` → `Err` с сообщением
    о том, что `cwd` больше не задаётся в конфиге (использовать `--cwd`
    или process cwd);
  - `config_default` на временном каталоге, когда файл отсутствует →
    создаёт `harness.json5` из шаблона и возвращает `valid: true`;
  - `config_default` на временном каталоге с уже существующим валидным
    конфигом → возвращает `valid: true` и redacted текст (не перезаписывает
    существующий файл).
  Для временных файлов использовать `std::env::temp_dir()` и уникальные
  имена (по аналогии с существующими тестами проекта). Для `config_default`
  потребуется `#[tokio::test]` и сконструированный `ServerState`
  (см. `crates/cli-harness/src/server/state.rs`). Новые внешние
  dev-зависимости не добавлять.

## Проверка корректности рефакторинга:
- `cargo test -p cli-harness` — все тесты проходят, включая новые.
- `cargo clippy -p cli-harness --all-targets` — предупреждения
  `collapsible_if` и `unnecessary_lazy_evaluations` больше не выдаются.
- `cargo clippy -p interactions --all-targets` — предупреждение
  `manual_pattern_char_comparison` больше не выдаётся.
- `cargo check --workspace` — без ошибок (после изменения сигнатуры
  `AgentLoop::new` все call-site'ы обновлены).

## Scope
Минимально необходимые изменения:

- `crates/cli-harness/src/cli.rs`
- `crates/cli-harness/src/server/mod.rs`
- `crates/cli-harness/src/server/config.rs`
- `crates/cli-harness/src/main.rs`
- `crates/cli-harness/src/server/run.rs`
- `crates/interactions/src/cli.rs`
- `crates/agent-loop/src/agent_loop.rs` (только сигнатура `new` и doc-комментарий)

Справочные материалы для чтения:

- `WiKi/todo/phase-7/pm-consulting report 3/report from refactoring manager.md`
- `crates/cli-harness/src/server/state.rs`
- `crates/agent-loop/src/agent_loop.rs`
- текущие исходники `crates/cli-harness/src/server`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 8-cli-harness-server-tests-and-clippy → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
