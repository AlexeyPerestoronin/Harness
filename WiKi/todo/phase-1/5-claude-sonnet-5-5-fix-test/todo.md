# Роль
Ты профессиональный разработчик (`см. WiKi/harness/roles/senior-developer.md`).

# Задача на исправление: Поддержка Claude Sonnet 5.5

Необходимо исправить тест `claude_sonnet_5_5` в `crates\harness\src\agents_live_tests\mod.rs`:
```rs
#[test]
#[ignore = "live API; cargo test -p harness -- --ignored --nocapture --test-threads=1"]
fn claude_sonnet_5_5() {
    run_live("claude-sonnet-5-5.json5");
}
```

Сейчас при запуске теста появляется ошибка:
```
Executing task: C:\Users\user\.cargo\bin\cargo.EXE test --package harness --bin harness -- agents_live_tests::claude_sonnet_5_5 --exact --nocapture --include-ignored 

   Compiling harness v1.0.0 (C:\MyLife\Harness\crates\harness)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 4.03s                                                                                                                                                                                                                                                                                                                                                                                                     
     Running unittests src\main.rs (target\debug\deps\harness-485c4c4687a9722a.exe)

running 1 test
API key for Claude: <user input>
live test log: C:/MyLife/Harness\.log/live\2026-10-17 22-33_Claude_Claude Sonnet 5.5\log.md

# Agent-loop session:
- agent: Claude Claude Sonnet 5.5
- iteration limit: 10
- tokens limit: 32000
- usd limit: 0.1
- start time: 2026-10-17 22:33

# Iterations:

## Iteration №1: (tokens = 0; usd = 0)
user prompt:
```
Внеси небольшое изменение в файл crates/harness/src/agents_live_tests/test-file.txt: допиши в конец одну строку `live-test-ok`. Затем сразу верни файл к исходному содержимому (убери эту строку). Другие файлы не трогай. Когда файл снова в исходном виде — остановись.
```
thread 'agents_live_tests::claude_sonnet_5_5' (7768) panicked at crates\harness\src\agents_live_tests\mod.rs:121:29:
live session failed: Claude HTTP 400 Bad Request: {"type":"error","error":{"type":"invalid_request_error","message":"tools.13.custom.input_schema.type: Field required"},"request_id":"req_011Cg88ct9CCLTiaJQcFVT3E"}
stack backtrace:
   0: __rustc::rust_begin_unwind
             at /rustc/48a229ceaefd4985c50990b14116b6d856af0985/library\std\src/panicking.rs:679:5
   1: core::panicking::panic_fmt
             at /rustc/48a229ceaefd4985c50990b14116b6d856af0985/library\core\src/panicking.rs:80:14
   2: harness::agents_live_tests::run_live::{closure#1}
             at .\crates\harness\src\agents_live_tests\mod.rs:121:29
   3: <core::result::Result<(), alloc::string::String>>::unwrap_or_else::<harness::agents_live_tests::run_live::{closure#1}>
             at C:\Users\user\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib/rustlib/src/rust\library\core\src\result.rs:1620:23
   4: harness::agents_live_tests::run_live
             at .\crates\harness\src\agents_live_tests\mod.rs:121:10
   5: harness::agents_live_tests::claude_sonnet_5_5
             at .\crates\harness\src\agents_live_tests\mod.rs:158:5
   6: harness::agents_live_tests::claude_sonnet_5_5::{closure#0}
             at .\crates\harness\src\agents_live_tests\mod.rs:157:23
   7: <harness::agents_live_tests::claude_sonnet_5_5::{closure#0} as core::ops::function::FnOnce<()>>::call_once
             at C:\Users\user\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib/rustlib/src/rust\library\core\src\ops\function.rs:250:5
   8: <fn() -> core::result::Result<(), alloc::string::String> as core::ops::function::FnOnce<()>>::call_once
             at /rustc/48a229ceaefd4985c50990b14116b6d856af0985/library\core\src\ops/function.rs:250:5
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
test agents_live_tests::claude_sonnet_5_5 ... FAILED

failures:

failures:
    agents_live_tests::claude_sonnet_5_5

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 20 filtered out; finished in 10.63s

error: test failed, to rerun pass `-p harness --bin harness`

 *  The terminal process "C:\Users\user\.cargo\bin\cargo.EXE 'test', '--package', 'harness', '--bin', 'harness', '--', 'agents_live_tests::claude_sonnet_5_5', '--exact', '--nocapture', '--include-ignored'" terminated with exit code: 101. 
 *  Terminal will be reused by tasks, press any key to close it. 
```

Для проверки теста используй тестовый ключ из API_KEY_CLAUDE чтобы избавиться от необходимости ввода на время проверки.
Когда тест будет зелёным, необходимо вернуть пользовательский ввод.


## Проверка корректности решения
1. Требуемый тест должен проходить.

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что-то какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Результаты — в `result.md` рядом с `todo.md`, формат `было→стало(почему)`.
4. Действия с моей стороны — в `note.md` рядом с `todo.md`.
