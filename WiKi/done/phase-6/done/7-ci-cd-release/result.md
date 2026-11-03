# Результат: CI/CD и релизные артефакты Phase 6

## Изменения

### `.github/workflows/ci.yml`

**Было**

- push только в `master`;
- матрица из двух ОС: `ubuntu-latest`, `windows-latest`;
- `cargo build --release --workspace`;
- артефакты `harness-<version>` / `harness-<version>.exe` с путями `target/release/harness` / `target/release/harness.exe`;
- не было web-сборки, деплоя Pages и Release.

**Стало**

- push в `master` + теги `v*`, `harness-*`;
- матрица из трёх ОС: `ubuntu-latest`, `windows-latest`, `macos-latest`;
- `cargo build --release -p cli-harness`;
- артефакты и пути:
  - Windows: `harness-<version>.exe`, `target/release/cli-harness.exe`;
  - Linux: `harness-<version>-linux`, `target/release/cli-harness`;
  - macOS: `harness-<version>-macos`, `target/release/cli-harness`;
- сохранены чтение версии из `[workspace.package].version`, удаление предыдущих артефактов с тем же именем и `actions/upload-artifact@v4`; regex имени артефакта расширен под суффиксы `-linux`/`-macos`;
- отдельная job `web`:
  - установка Rust и target `wasm32-unknown-unknown`;
  - установка Trunk (`taiki-e/install-action@v2`);
  - `cd crates/web-harness && trunk build --release`;
  - загрузка `crates/web-harness/dist` через `actions/upload-pages-artifact@v3`;
- job `deploy-pages`:
  - только на push в `master`;
  - `pages: write`, `id-token: write`;
  - `actions/deploy-pages@v4`;
- job `release`:
  - только на теги `v*` / `harness-*`;
  - скачивает три нативных артефакта и прикрепляет их к Release через `softprops/action-gh-release@v2`.

**Почему**

Фаза должна давать готовые артефакты: нативные `harness`/`harness.exe` для трёх ОС, `harness.wasm`/`dist` для GitHub Pages и GitHub Release по тегу, не пытаясь собирать `web-harness` нативно.

### `README.md` и `README.ru.md`

**Было**

- устаревший workspace (`crates/harness` и `--workspace`);
- не описаны серверный режим и web-клиент;
- CI описан как сборка только на Windows/Linux.

**Стало**

- актуальный workspace и команда `cargo build --release -p cli-harness`;
- описание серверного режима: `harness.exe --serve`, флаги `--port`, `--host`, `--conf`, `--cwd`, `--auth-token`, `--tls-cert`, `--tls-key`, порт по умолчанию `4067`;
- печать Bearer-токена в stdout и обязательный заголовок `Authorization: Bearer <token>`;
- описание web-клиента: `harness.wasm` на GitHub Pages, подключение по HTTPS, polling 10 секунд, отсутствие лога/выбора агента/ввода ключа на клиенте;
- инструкция доверия самоподписанному сертификату в браузере;
- интеграция `web-harness` через Cargo/Git с фичей `fs-panel`;
- актуальное описание CI.

### `crates/cli-harness/help.txt`

**Было**: серверные флаги уже присутствовали, но не было примеров.

**Стало**: добавлена секция `EXAMPLES`:

```text
EXAMPLES:
    harness.exe --serve
    harness.exe --serve --host 127.0.0.1 --port 4067
    harness.exe --serve --auth-token <token> --conf harness.json5
```

## Проверка

1. `cargo test --workspace` — успех; `web-harness` исключён из workspace и не собирается нативно.
2. `cargo build --release -p cli-harness` — успех.
3. `cd crates/web-harness && trunk build --release` — успех.
4. `git diff` по `.github/workflows/ci.yml`, `README.md`, `README.ru.md`, `crates/cli-harness/help.txt` просмотрен.
