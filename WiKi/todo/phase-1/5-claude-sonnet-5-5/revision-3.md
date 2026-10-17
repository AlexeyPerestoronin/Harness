# Задача на доработку:
Необходимо доработать задачу `WiKi\todo\phase-1\5-claude-sonnet-5-5\todo.md` на третье итерации доработок (первые итерации см. `WiKi\todo\phase-1\5-claude-sonnet-5-5\revision-1.md` и `WiKi\todo\phase-1\5-claude-sonnet-5-5\revision-2.md`) по следующим пунктам:

1. исправить тест live_availability для Claude (см. `crates\harness\src\agents\claude.rs`) → сейчас имеется ошибка:
```
running 1 test
API key for Claude: <API-KEY>
live test log: \\?\C:\MyLife\Harness\.log\live\2026-10-17 21-52-18_Claude_Claude Sonnet 5.5\log.md

user prompt:
```
Ты доступен? Какой ты AI?
```
thread 'agents::claude::tests::live_availability' (2056) panicked at crates\harness\src\agents\claude.rs:359:9:
Claude live request failed: Some("Claude HTTP 400 Bad Request: {\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"max_tokens: 500000 > 128000, which is the maximum allowed number of output tokens for claude-sonnet-5-5\"},\"request_id\":\"req_011Cg85Sr9zzWDo4vsEiCjzS\"}")
stack backtrace:
   0: __rustc::rust_begin_unwind
             at /rustc/48a229ceaefd4985c50990b14116b6d856af0985/library\std\src/panicking.rs:679:5
   1: core::panicking::panic_fmt
             at /rustc/48a229ceaefd4985c50990b14116b6d856af0985/library\core\src/panicking.rs:80:14
   2: harness::agents::claude::tests::live_availability
             at .\src\agents\claude.rs:359:9
   3: harness::agents::claude::tests::live_availability::{closure#0}
             at .\src\agents\claude.rs:354:27
   4: <harness::agents::claude::tests::live_availability::{closure#0} as core::ops::function::FnOnce<()>>::call_once
             at C:\Users\user\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib/rustlib/src/rust\library\core\src\ops\function.rs:250:5
   5: <fn() -> core::result::Result<(), alloc::string::String> as core::ops::function::FnOnce<()>>::call_once
             at /rustc/48a229ceaefd4985c50990b14116b6d856af0985/library\core\src\ops/function.rs:250:5
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
test agents::claude::tests::live_availability ... FAILED

failures:

failures:
    agents::claude::tests::live_availability

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 18 filtered out; finished in 11.02s

error: test failed, to rerun pass `-p harness --bin harness`

 *  The terminal process "C:\Users\user\.cargo\bin\cargo.EXE 'test', '--package', 'harness', '--bin', 'harness', '--', 'agents::claude::tests::live_availability', '--exact', '--nocapture', '--include-ignored'" terminated with exit code: 101. 
 *  Terminal will be reused by tasks, press any key to close it. 
```
Для проверки работоспособности используй тестовый ключ в файле `API_KEY_CLAUDE`.

2. Все `live API` тесты перенести в отдельный модуль `crates/harness/src/agents_live_tests` и изменить логику тестов, максимально приблизив её к рабочей:
      - тестовый запуск для агента должен использовать реальный файл конфигурации адаптированный для простого тестового сценария `crates\harness\src\agents_live_tests\test-settings.json5`;
      - смысл тестов должен сводиться к тому, чтобы внести небольшие изменения в тестовый файл `crates\harness\src\agents_live_tests\test-file.txt` и откатить их.

3. Сделать так, чтобы команда `cargo build --release` копировала получившейся артефакт в корень проекта.