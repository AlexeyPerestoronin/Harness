# Изменение pipeline: GitHub release action

## До

```mermaid
flowchart TD
    A[push: branch / v* / harness-*] --> B[ci job]
    B --> C[upload artifacts]
    C --> D[release job: любой тег]
    D --> E[download artifacts]
    E --> F[prepare assets без проверки полноты и без sha256]
    F --> G[softprops/action-gh-release v2]
```

## После

```mermaid
flowchart TD
    A[push: branch / v* / harness-*] --> B[ci job]
    B --> C[upload artifacts: harness-version-windows.exe / -linux / -macos]
    C --> D{release job: startsWith github.ref refs/tags/v}
    D -- нет --> X[релиз не создаётся]
    D -- да --> E[read Cargo.toml version]
    E --> F{tag_version == version?}
    F -- нет --> Y[fall job]
    F -- да --> G[download artifacts]
    G --> H[prepare assets]
    H --> I{все три файла существуют?}
    I -- нет --> Z[fall job]
    I -- да --> J[sha256sum -> release/*.sha256]
    J --> K[softprops/action-gh-release v2: generate_release_notes + files: release/*]
```

## Почему

- `release` job больше не срабатывает на любой тег: теги `harness-*` остаются
  только CI-сборкой, а релиз создаётся исключительно для версионных тегов `v*`.
- Версия тега сверяется с `[workspace.package].version`, чтобы в релиз нельзя
  было опубликовать дистрибутивы с несовпадающей версией.
- Перед вызовом action проверяется полнота ассетов и считаются контрольные
  суммы — релиз не создаётся, если любой из трёх бинарников отсутствует.
- `generate_release_notes: true` использует нативный механизм GitHub вместо
  ручной подготовки release notes.
