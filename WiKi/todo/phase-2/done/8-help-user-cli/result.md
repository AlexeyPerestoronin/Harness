# result

`crates/tools/help-user.txt`: не было файла → английская справка по модулям `fs`/`git`/`terminal`/`user`, секции Variables и Access model (человек узнаёт параметры tools не только из шаблона/кода).

`crates/tools/src/lib.rs`: не было константы → `pub const HELP_USER = include_str!("../help-user.txt")` + тест, что текст содержит все модули, ключи conf и имена `descriptors()` (вшить справку в бинарник и не дать ей разъехаться с кодом).

`crates/harness/src/cli.rs`: только `--help` → `CliAction::HelpTools` и флаг `--help-tools` с тем же приоритетом, что `--help`; при обоих флагах побеждает `--help` (как в ТЗ).

`crates/harness/src/main.rs`: не печатал tools-справку → ветка `HelpTools` печатает `tools::HELP_USER` с завершающим `\n` и выходит 0, без conf/ключей/сети.

`crates/harness/help.txt`: USAGE/FLAGS без `--help-tools` → добавлены строка USAGE и описание флага.
