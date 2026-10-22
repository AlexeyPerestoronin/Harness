# Результат

cli.rs: не было `--version` → `CliAction::Version`, флаг `--version` (без `-V`), приоритет `--help` → `--help-tools` → `--version`, `version_line()` из `env!("CARGO_PKG_VERSION")`, тесты (единственный источник версии — workspace, флаг как у `--help`, без хардкода)

main.rs: не было ветки Version → печатает `version_line()` в stdout и выходит 0 (конфиг/сеть не нужны)

help.txt: не было `--version` → USAGE `harness.exe --version`, FLAGS с текстом `Print the application version and exit with code 0.`

проверка: `cargo test --workspace` ok; `cargo run -p harness -- --version` → `harness 1.1.0` (как `[workspace.package].version`)
