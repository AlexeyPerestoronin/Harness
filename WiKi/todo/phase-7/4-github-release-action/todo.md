# Задача на разработку: GitHub release action

Доработать `.github/workflows/ci.yml` в соответствии с утверждённым планом phase-7 (consulting 3): релиз создаётся только для тегов `v*`, версия тега сверяется с `[workspace.package].version`, артефакты именуются целевым образом, к релизу прикладываются бинарники и их `.sha256`. Изменения ограничиваются одним workflow-файлом.

Зачем: чтобы получаемые на GitHub Actions дистрибутивы публиковались через `Create GitHub Release` с корректными именами ассетов, контрольными суммами и только для согласованных с версией Cargo тегов.

Ожидаемый результат: обновлённый `.github/workflows/ci.yml` корректно собирает, загружает и скачивает артефакты с целевыми именами, а release job создаёт GitHub Release только для тегов `v*` с версией, совпадающей с `Cargo.toml`.

## Функциональные требования

### Именование артефактов (upload и download синхронно)

1. В `ci` job шаг `Artifact name and path` для Windows задаёт имя `harness-<version>-windows.exe` (путь `target/release/cli-harness.exe` без изменений).
2. Linux: `harness-<version>-linux`; macOS: `harness-<version>-macos` — без изменений.
3. Шаг `Upload release binary` продолжает использовать `${{ steps.artifact.outputs.name }}`.

### Строгая валидация имени артефакта

4. В шаге «Delete previous artifacts with the same name» заменить текущий regex на строгий:
   ```regex
   ^harness-[0-9]+\.[0-9]+\.[0-9]+(-linux|-macos|\.exe|-windows\.exe)$
   ```
5. Допускаются только:
   - `harness-<version>-windows.exe`;
   - `harness-<version>-linux`;
   - `harness-<version>-macos`;
   - legacy `harness-<version>.exe`.
6. Отклоняются: `harness-<version>-windows` (без `.exe`), `harness-<version>-linux.exe`, `harness-<version>-macos.exe`, бессуффиксное имя.

### Триггер релиза

7. Release job запускается только для тегов `v*`:
   ```yaml
   if: startsWith(github.ref, 'refs/tags/v')
   ```
8. Теги `harness-*` остаются CI-сборкой без создания релиза.

### Сверка версии

9. В `release` job после чтения `[workspace.package].version` добавить проверку:
   ```bash
   tag_version="${GITHUB_REF_NAME#v}"
   if [ "$tag_version" != "$version" ]; then
     echo "Tag '$GITHUB_REF_NAME' does not match Cargo.toml version '$version'" >&2
     exit 1
   fi
   ```

### Скачивание артефактов

10. `Download Windows artifact`: name `harness-<version>-windows.exe`.
11. Linux/macOS download: имена без изменений.

### Подготовка ассетов

12. Подготовить файлы:
    - `release/harness-<version>-windows.exe`
    - `release/harness-<version>-linux`
    - `release/harness-<version>-macos`
13. До вызова action проверить существование всех трёх файлов; при отсутствии любого — упасть с ошибкой.
14. Посчитать `sha256sum` и записать в `release/*.sha256`.

### Создание релиза

15. `softprops/action-gh-release@v2` с параметрами:
    - `generate_release_notes: true`;
    - `files: release/*`.
16. `permissions: contents: write` у release job сохраняется; связь `needs: ci` сохраняется.

## Проверка корректности решения

- Проверить синтаксис YAML и корректность `${{ ... }}` выражений визуально/локально.
- Пройти по шагам workflow и убедиться:
  - `ci` job на Windows загружает `harness-<version>-windows.exe`;
  - шаг удаления старых артефактов принимает `harness-<version>-windows.exe` и legacy `harness-<version>.exe`, но отклоняет `harness-<version>-windows`, `harness-<version>-linux.exe`, `harness-<version>-macos.exe` и бессуффиксное имя;
  - `release` job: `if` — `startsWith(github.ref, 'refs/tags/v')`; тег `v<X.Y.Z>` с версией, не совпадающей с `Cargo.toml`, роняет job; тег `harness-*` не создаёт релиз;
  - `release` job скачивает все три артефакта под целевыми именами;
  - `Prepare release assets` падает, если любого из трёх бинарников нет;
  - `sha256sum` создаёт `release/*.sha256`;
  - релиз создаётся через `softprops/action-gh-release@v2` с `generate_release_notes: true` и `files: release/*`.
- Убедиться, что `[workspace.package].version` в корневом `Cargo.toml` читается корректно (текущее значение `2.1.0`).

## Scope

Минимально необходимые изменения:

- `.github/workflows/ci.yml`

Справочные материалы для чтения:

- `WiKi/todo/phase-7/pm-consulting report 3/base.md`
- `WiKi/todo/phase-7/pm-consulting report 3/arch.md`
- `WiKi/todo/phase-7/pm-consulting report 3/tech.md`
- `Cargo.toml` (корень проекта)
- текущий `.github/workflows/ci.yml`

# Дополнительные указания
1. Экономь токены: читай минимум необходимого; правки вноси через patch.
2. Если что-то не получается со второго раза, или ты понимаешь, что контекст задачи сильно разрастается относительно цели задачи в минимальном воплощении, или что какая-то информация не дана, но является важной для правильной реализации поставленной задачи → не фантазируй и не додумывай за меня → остановись и задай вопрос → я подскажу и направлю.
3. Отчёт о проделанной работе согласно правилам целевой роли.
4. По завершении работы необходимо зафиксировать ВСЕ изменения в Git со специальным комментарием: `senior-developer: phase-7 → 4-github-release-action → iteration-<task-solution-iteration>: <very short comment about essence of the task>`
