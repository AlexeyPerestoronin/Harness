# Результаты: GitHub release action

## Было

- `ci` job на Windows загружал артефакт с именем `harness-<version>.exe`.
- Валидация имени артефакта допускала лишние комбинации:
  `harness-<version>-linux.exe`, `harness-<version>-macos.exe` и бессуффиксное имя.
- `release` job запускался для любого тега (`startsWith(github.ref, 'refs/tags/')`).
- Версия тега не сверялась с `[workspace.package].version` из `Cargo.toml`.
- Windows-артефакт скачивался под старым именем `harness-<version>.exe`.
- В `Prepare release assets` не было проверки полноты ассетов и не считались `.sha256`.
- `softprops/action-gh-release@v2` вызывался без `generate_release_notes`.

## Стало

- `ci` job на Windows загружает артефакт `harness-<version>-windows.exe`.
- Валидация имени артефакта заменена на строгий regex:
  `^harness-[0-9]+\.[0-9]+\.[0-9]+(-linux|-macos|\.exe|-windows\.exe)$`.
- `release` job запускается только для тегов `v*`:
  `if: startsWith(github.ref, 'refs/tags/v')`.
- В `release` job после чтения версии из `Cargo.toml` добавлена сверка
  `tag_version="${GITHUB_REF_NAME#v}"` с `version`; при несовпадении job падает.
- Windows-артефакт скачивается под именем `harness-<version>-windows.exe`.
- `Prepare release assets` формирует три целевых файла в `release/`, проверяет
  их существование и считает `sha256sum` для каждого в `release/*.sha256`.
- `softprops/action-gh-release@v2` вызывается с `generate_release_notes: true`
  и `files: release/*`.

## Почему

- Имена ассетов должны соответствовать утверждённому плану phase-7 и быть
  одинаковыми на этапах upload/download.
- Строгий regex исключает несуществующие платформенные комбинации и
  оставляет только целевые имена плюс legacy `harness-<version>.exe`.
- Релиз создаётся только для согласованных с версией Cargo тегов `v*`;
  теги `harness-*` остаются CI-сборкой без релиза.
- Контрольные суммы и проверка полноты ассетов гарантируют, что в релиз не
  попадут неполные дистрибутивы.
