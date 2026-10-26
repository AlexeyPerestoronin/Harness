# 4-conf-template-file

Шаблон `--conf-new` собирался `format!` в `template.rs` из констант `keys` → текст живёт в `crates/conf/settings-template.json5` и встраивается через `include_str!("../settings-template.json5")` (править json5 как файл, без десятков подстановок в Rust).

Пользователь заполнял плейсхолдеры по всему шаблону → первая секция `vars` с инструкцией «Edit only this section»; ниже значения как `${vars:<name>}` (пути внутри строк, числа снаружи).

`PLACEHOLDER_*` и `DEFAULT_{TOKEN,ITERATION,USD,EXECUTION}_LIMIT` использовались только сборщиком шаблона → удалены; `DEFAULT_LOG_FOLDER` оставлен (фолбэк загрузчика).

Согласованность ключей шаблона и `keys` больше не следует из `format!` → тесты: substitute+json5, все vars объявлены и использованы, ключи загрузчиков есть в шаблоне как `"<key>"`, `conf::load` после подстановки реальных путей.

`load.rs` / `vars.rs` / `help.txt` не менялись (CLI и защита от перезаписи те же).
