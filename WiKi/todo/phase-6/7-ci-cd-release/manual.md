# Ручные действия

1. **Включить GitHub Pages на Actions.**
   В репозитории: Settings → Pages → Source → `GitHub Actions`.
   Без этого job `deploy-pages` отработает, но публикация `dist` не будет доступна по URL.

2. **Локальная проверка серверного режима.**
   - `cargo run -p cli-harness -- --serve` (или `target/release/cli-harness --serve`);
   - скопировать Bearer-токен из stdout сервера;
   - в браузере открыть `https://localhost:4067/api/v1/health` и принять исключение самоподписанного сертификата;
   - открыть `harness.wasm` с GitHub Pages, ввести адрес `https://localhost:4067` и токен.

3. **Создание GitHub Release.**
   - создать тег `v*` или `harness-*` (например, `v1.5.0`) и запушить его;
   - CI соберёт три нативных бинарника и прикрепит их к Release автоматически.
