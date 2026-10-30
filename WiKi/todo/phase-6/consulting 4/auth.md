# Phase 6. Аутентификация сервера

> Документ описывает механизм аутентификации `harness.exe --serve`.
> Смежные документы: `base.md`, `arch.md`, `tech.md`, `api.md`, `ui.md`.

---

## 1. Требования

1. Все эндпоинты `/api/v1/*`, кроме `GET /health`, требуют аутентификации.
2. Механизм реализуется в отдельном crate `auth`.
3. Механизм должен быть простым и не требовать пользовательских паролей/сессий.
4. Токен не должен позволять стороннему origin выполнять запросы; CORS при этом запрещён по умолчанию.

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
    extractor.rs    # axum::extract::FromRequestParts -> AuthToken
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

### 3.2. Axum-экстрактор

```rust
#[async_trait::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ErrorBody>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        // 1. извлечь Authorization
        // 2. выделить "Bearer <token>"
        // 3. извлечь AuthToken из state
        // 4. выполнить constant-time verify
    }
}
```

Ответ при ошибке — `401`:

```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "missing or invalid bearer token"
  }
}
```

---

## 4. Защита маршрутов

```rust
Router::new()
    .route("/api/v1/health", get(handlers::health)) // без auth
    .route("/api/v1/agents", get(handlers::agents))
    // ...
    .layer(middleware::from_fn_with_state(state.clone(), auth::require_auth))
```

Исключение — `GET /api/v1/run/events`: поскольку браузерный `EventSource` не поддерживает заголовок `Authorization`, токен для этого эндпоинта принимается query-параметром `?auth_token=<token>` или `?ticket=<short-lived-ticket>`. Middleware проверяет query-параметр только для SSE-маршрута.

Все защищённые маршруты используют единый middleware, который проверяет `Authorization` до вызова обработчика.

---

## 5. Поведение клиента

### 5.1. Standalone `harness.wasm`

1. `ConnectDialog` запрашивает адрес сервера и токен.
2. Токен сохраняется только в `localStorage`/`sessionStorage` текущего origin.
3. При получении `401` клиент сбрасывает сохранённый токен и снова показывает `ConnectDialog`.

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
| Токен передан в URL и попал в логи/историю | Не используется: токен передаётся только в заголовке |
| Локальный процесс пользователя читает токен из stdout | Приемлемо: локальный пользователь уже имеет доступ к машине |
| Токен потерян при рестарте сервера | Ожидаемо: пользователь вводит новый токен; при необходимости использует `--auth-token` |
| Brute-force токена | Неактуально: 256 бит энтропии; запросы локальные |

---

## 8. Тесты

1. `GET /api/v1/health` без токена → `200`.
2. `GET /api/v1/agents` без токена → `401 UNAUTHORIZED`.
3. `GET /api/v1/agents` с неверным токеном → `401 UNAUTHORIZED`.
4. `GET /api/v1/agents` с верным токеном → `200`.
5. Заголовки `Bearer` с некорректной схемой → `401`.
6. Constant-time сравнение: модульный тест на совпадение/несовпадение.
7. CORS: при отсутствии `server.cors-allowed-origins` ответ не содержит `Access-Control-Allow-Origin`.
