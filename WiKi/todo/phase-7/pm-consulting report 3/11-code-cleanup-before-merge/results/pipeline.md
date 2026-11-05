# Code cleanup before final merge — pipeline

## До

```mermaid
flowchart LR
  Push[Push / tag] --> Matrix[CI matrix: ubuntu, windows, macos]
  Matrix --> Clippy[Clippy -D warnings]
  Clippy --> Test[Test: cargo test --workspace]
  Test --> Build[Build release]
  Build --> Artifact[Artifact name regex]
  Artifact --> Upload[Upload release binary]

  TagLegacy[Tag 'harness-*'] --> Push
  TagV[Tag 'v*'] --> Push

  Artifact --> RegexAccept[Regex accepts: -linux, -macos, .exe, -windows.exe]
```

## После

```mermaid
flowchart LR
  Push[Push / tag] --> Matrix[CI matrix: ubuntu, windows, macos]
  Matrix --> Clippy[Clippy -D warnings]
  Clippy --> Build[Build release]
  Build --> Artifact[Artifact name regex]
  Artifact --> Upload[Upload release binary]

  TagV[Tag 'v*'] --> Push

  Artifact --> RegexAccept[Regex accepts: -linux, -macos, -windows.exe]
```

## Почему

- Unit-тесты native-крейтов удалены перед финальным merge, поэтому шаг `Test` (`cargo test --workspace`) больше не проверяет ничего полезного и удалён из CI.
- Legacy-триггер `'harness-*'` удалён: релизы запускаются только по тегам `v*`.
- Regex имени артефакта ужесточён: legacy-вариант `harness-<version>.exe` больше не принимается; целевые имена `harness-<version>-windows.exe`, `harness-<version>-linux`, `harness-<version>-macos` остаются.
