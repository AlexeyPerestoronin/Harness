# Phase-7, задача 1: CWD first + deprecated-dir-patterns — архитектура

## До

```mermaid
flowchart TD
    CONF[harness.json5] -->|cwd| LOAD[conf::load]
    CONF -->|agent.config от conf_dir| LOAD
    LOAD -->|resolve от conf_dir или settings.cwd| PATHS[Конфигурационные пути]
    LOAD -->|api-key от process cwd| MODEL[model.json5]
```

- Источник CWD — файл конфига.
- Разные правила резолва для разных групп путей.
- Нет корневой валидации и нет glob-паттернов.

## После

```mermaid
flowchart TD
    CALLER[cli-harness / server] -->|cwd| LOAD[conf::load / load_with / load_server_config]
    LOAD -->|ensure_conf_in_cwd| READ[Чтение и парсинг harness.json5]
    READ -->|root validation| ROOT[vars, server, agent, tools; cwd запрещён]
    LOAD --> RESOLVE[resolve_in_cwd]
    RESOLVE --> INSIDE{путь внутри CWD?}
    INSIDE -->|да| PATHS[agent.config, fs, terminal, skills, roles, log-folder]
    INSIDE -->|нет| ERR[ConfError]
    LOAD --> TLS[server.tls-* без проверки внутри CWD]
    LOAD --> KEY[api-key без проверки внутри CWD]
    LOAD --> GIT[git.executable без резолва]
    READ --> GLOB[glob::regex_source]
    GLOB --> REGEX[Regex::new]
```

- CWD приходит только из вызывающего кода.
- Единый помощник `resolve_in_cwd` для всех рабочих путей конфига.
- Осознанные исключения вынесены из общего правила.
- `deprecated-dir-patterns` валидируются и компилируются в `conf`.

## Почему

- Единственный источник истины CWD — процесс запуска, а не файл.
- Один помощник резолва упрощает проверку «внутри CWD» и делает поведение предсказуемым.
- Отдельный модуль `glob` владеет трансляцией glob → regex, а `load_fs` только валидирует и компилирует.
