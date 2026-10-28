# Ручные проверки

> Код уже проверен автоматически: `cargo check --workspace` и `cargo test --workspace` проходят.
> Ниже — шаги для ручного подтверждения границы CWD.

## Подготовка
1. Собрать проект: `cargo build --workspace`.
2. Подготовить тестовый CWD, отличный от process cwd, например:
   - `C:\MyLife\Harness\WiKi\todo\phase-5\04-enforce-cwd-boundary\manual-cwd`
   - положить в него файл `inside.txt`.
   - рядом с ним (вне CWD) положить `outside.txt`.

## Проверка fs-инструмента
1. Запустить harness с conf, где:
   - `cwd` указывает на `manual-cwd`;
   - `tools.fs.available-dirs = ["w:."]`;
   - `tools.fs.deprecated-dirs = []`.
2. Попросить модель прочитать `inside.txt` — должно работать.
3. Попросить модель прочитать/записать `../outside.txt` или `../../../outside.txt` — инструмент должен вернуть ошибку «path ... is outside root cwd».
4. Убедиться, что `outside.txt` не был прочитан/создан.

## Проверка request_command_shell_execution
1. В том же conf включить `tools.user = {}` и `tools.terminal` с `command-execution-limit`.
2. Попросить модель выполнить shell-команду с `cwd = "../"` или абсолютным путём вне CWD.
3. Инструмент должен отклонить запрос до подтверждения пользователем (ошибка «outside root cwd»).

## Проверка request_read_access_for / request_write_access_for
1. Попросить модель запросить доступ к `../outside.txt`.
2. Инструмент должен вернуть ошибку, не спрашивая пользователя, и не добавить путь в allow-list.

## Проверка log-folder
1. В conf задать `agent.log-folder = "../outside-logs"`.
2. Запустить новую сессию.
3. Запуск должен завершиться ошибкой `invalid log-folder '...'`, каталог `outside-logs` вне CWD не должен создаться.

## Проверка конфликтов tools.fs при CWD != process cwd
1. В conf задать:
   - `cwd = manual-cwd`;
   - `tools.fs.available-dirs = ["w:src"]`;
   - `tools.fs.deprecated-dirs = ["src"]`.
2. Загрузка конфигурации должна завершиться ошибкой конфликта `available-dirs` / `deprecated-dirs`.
3. Те же значения, но при CWD, совпадающем с process cwd, дают тот же результат — теперь конфликт не зависит от process cwd.
