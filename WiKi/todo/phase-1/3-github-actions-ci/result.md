# Результат: GitHub Actions CI (Windows + Linux)

- CI не было → `.github/workflows/ci.yml`: `on.push.branches: master`, матрица `ubuntu-latest` + `windows-latest`, шаги `actions/checkout@v4` + `dtolnay/rust-toolchain@stable` + `cargo test --workspace` + `cargo build --release --workspace` (кроссплатформенные регрессии не ловились автоматически)
- бинарники только из исходников → два артефакта `harness-<X.Y.Z>` (Linux, `target/release/harness`) и `harness-<X.Y.Z>.exe` (Windows, `target/release/harness.exe`); `<X.Y.Z>` читается из `[workspace.package].version` корневого `Cargo.toml`, без хардкода (сейчас `1.0.0`)
- повторный push той же версии копил бы копии → перед upload удаляются прежние артефакты с тем же именем (`gh api` + `upload-artifact@v4` `overwrite: true`)

Код приложения не менялся.

Проверка локально (Windows): `cargo test --workspace` — conf 13, harness 12, tools 30; `cargo build --release --workspace` — ok.
Прогон на GitHub подтвердит зелёный статус после push в `master`.
