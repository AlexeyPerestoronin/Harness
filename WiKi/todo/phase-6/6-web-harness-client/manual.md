# Ручные действия

## 1. Сборка standalone-клиента

В среде, где установлен Trunk и добавлен wasm-таргет:

```text
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cd crates/web-harness
trunk build --release
```

После успешной сборки появится каталог `crates/web-harness/dist`.

## 2. Ручная проверка standalone

1. Запустить сервер локально:
   ```text
   harness.exe --serve
   ```
   Токен будет напечатан в stdout сервера.
2. Открыть клиент:
   ```text
   cd crates/web-harness
   trunk serve
   ```
   либо открыть собранный `dist/index.html` через статический HTTPS-хостинг.
3. При необходимости передать адрес сервера query-параметром:
   ```text
   https://<origin>/?server=https://localhost:4067
   ```
4. Принять самоподписанный сертификат: открыть `https://localhost:4067/api/v1/health` и подтвердить исключение.
5. Ввести токен, запустить цикл, дождаться статуса; при появлении `pendingInput` ответить; проверить Stop и результат.
6. Проверить password-режим при `sensitive: true` (например, запрос API-ключа от сервера) и отсутствие клиентской эвристики по подстроке в `prompt`.

## 3. Проверка fs-panel (встраивание)

```toml
[dependencies]
web-harness = { path = "crates/web-harness", features = ["fs-panel"] }
```

В родительском приложении:

```rust
use web_harness::HarnessPanel;

#[component]
pub fn HostApp() -> impl IntoView {
    view! {
        <HarnessPanel
            base_url="https://localhost:4067".to_string()
            auth_token=Some("token".to_string())
            compact=true
        />
    }
}
```

В этом режиме панель дополнительно показывает вкладки «Конфигурация» и «Файлы».
