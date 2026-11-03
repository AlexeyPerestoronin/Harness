# Phase 6. Аутентификация сервера

> Документ описывает механизм аутентификации `harness.exe --serve`.
> Смежные документы: `base.md`, `arch.md`, `tech.md`, `api.md`, `ui.md`.

---

## 1. Требования

1. Все эндпоинты `/api/v1/*`, кроме `GET /health`, требуют аутентификации.
2. Механизм реализуется в отдельном crate `auth`.
3. Механизм должен быть простым и не требовать пользовательских паролей/сессий.
4. Токен не должен позволять стороннему origin выполнять запросы; CORS при этом запрещён по умолчанию.
5. Токен передаётся **только** в заголовке `Authorization`. Query-параметры, cookies и иные каналы не используются.

---

## 2. Модель токена

- Сервер при старте генерирует случайный токен: 32 байта, кодирование **base64url без `=`**.
- Токен печатается в stdout сервера.
- Токен хранится в памяти сервера (`ServerState`) и **не записывается** в файлы.
- Пользователь может задать фиксированный токен через CLI:

```text
harness.exe --serve --auth-token <token>
```

- Клиент передаёт токен в заголовке:

```text
Authorization: Bearer <token>
```

- Сравнение выполняется **constant-time**, чтобы исключить тайминг-атаки.

---

## 3. Состав crate `auth`

```
crates/auth/
  src/
    lib.rs          # re-exports
    token.rs        # AuthToken, generate, parse, verify
    middleware.rs   # axum-middleware require_auth
    error.rs        # AuthError -> 401 response
```

### 3.1. Контракт

```rust
#[derive(Clone, Debug)]
pub struct AuthToken {
    secret: [u8; 32],
}

impl AuthToken {
    pub fn generate() -> Self;
    pub fn parse(input: &str) -> Result<Self, AuthError>;
    pub fn verify(&self, candidate: &str) -> bool; // constant-time
}

#[derive(Debug)]
pub enum AuthError {
    MissingHeader,
    InvalidScheme,
    InvalidToken,
    Unauthorized,
}
```

### 3.2. Middleware — единственный механизм защиты

В отличие от варианта с extractor'ом, защита маршрутов реализуется **только** middleware'ом:

```rust
pub async fn require_auth(
    State(state): State<Arc<ServerState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorBody>)> {
    let token = extract_bearer(request.headers())?;
    if !state.auth_token.verify(token) {
        return Err(AuthError::Unauthorized.into_response());
    }
    Ok(next.run(request).await)
}
```

`extract_bearer` принимает ровно одну схему — `Bearer <token>`; отсутствие заголовка, другая схема или некорректный формат дают `401`.

Ответ при ошибке — `401`:

```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "missing or invalid bearer token"
  }
}
```

Middleware навешивается на весь protected-роутер одним слоем; обработчики не выполняют проверку токена повторно.

---

## 4. Защита маршрутов

```rust
Router::new()
    .route("/api/v1/health", get(handlers::health)) // без auth
    .route("/api/v1/run", post(handlers::start_run))
    .route("/api/v1/run/current", get(handlers::run_status))
    .route("/api/v1/run/stop", post(handlers::stop_run))
    .route("/api/v1/run/input", post(handlers::submit_input))
    // ... config, fs
    .layer(middleware::from_fn_with_state(state.clone(), auth::require_auth))
```

Исключений нет: SSE-эндпоинты отсутствуют, поэтому query-токены и `sse_ticket` не нужны. Все защищённые маршруты проверяются одинаково — только по заголовку `Authorization`.

---

## 5. Поведение клиента

### 5.1. Standalone `harness.wasm`

1. `ConnectDialog` запрашивает адрес сервера и токен.
2. Токен сохраняется только в `localStorage`/`sessionStorage` текущего origin.
3. При получении `401` клиент сбрасывает сохранённый токен и снова показывает `ConnectDialog`.
4. Токен добавляется в каждый REST-запрос как заголовок `Authorization`.

### 5.2. Встраиваемый плагин

Родительское приложение передаёт токен одним из способов:

```rust
// через prop
<HarnessPanel
    base_url="https://localhost:4067".to_string()
    auth_token=Some("...".to_string())
    compact=true
/>

// или программно
let gateway = HarnessGateway::new(
    "https://localhost:4067",
    Some("...".to_string()),
);
```

Токен не должен зашиваться в исходный код публичных страниц; для production-сборок рекомендуется получать его из переменных окружения сборки или из пользовательского ввода.

---

## 6. Взаимодействие с CORS

- CORS и аутентификация — разные слои защиты.
- CORS по умолчанию запрещён: сервер не отправляет `Access-Control-Allow-Origin`.
- Даже при явно разрешённом origin в `server.cors-allowed-origins` доступ остаётся защищён Bearer-токеном.
- Значение `server.cors-allowed-origins = ["*"]` в первой версии не поддерживается и должно отклоняться при валидации конфигурации.

---

## 7. Угрозы и ограничения

| Угроза | Статус |
|---|---|
| Случайный сторонний сайт читает `/api/v1/*` | Закрыто: CORS запрещён по умолчанию + Bearer-токен |
| Токен передан в URL и попал в логи/историю | Не используется: токен передаётся только в заголовке `Authorization` |
| Токен утекает через логи сервера | Низкий риск: сервер не логирует заголовок `Authorization` |
| Локальный процесс пользователя читает токен из stdout | Приемлемо: локальный пользователь уже имеет доступ к машине |
| Токен потерян при рестарте сервера | Ожидаемо: пользователь вводит новый токен; при необходимости использует `--auth-token` |
| Brute-force токена | Неактуально: 256 бит энтропии; запросы локальные |

---

## 8. Тесты

1. `GET /api/v1/health` без токена → `200`.
2. `GET /api/v1/run/current` без токена → `401 UNAUTHORIZED`.
3. `GET /api/v1/run/current` с неверным токеном → `401 UNAUTHORIZED`.
4. `GET /api/v1/run/current` с верным токеном → `200`.
5. Заголовки `Bearer` с некорректной схемой → `401`.
6. Токен в query-параметре (`?auth_token=...`) → `401`.
7. Constant-time сравнение: модульный тест на совпадение/несовпадение.
8. CORS: при отсутствии `server.cors-allowed-origins` ответ не содержит `Access-Control-Allow-Origin`.
