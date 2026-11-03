# Consulting 2 — Phase 7. Web UI

## Вывод
Phase-7 **не затрагивает** web-интерфейс (`crates/web-harness`) и не требует
изменений UI.

## Обоснование
- Задачи фазы относятся к native-части (`cli-harness`, `conf`, `tools`) и
  GitHub Actions.
- `web-harness` собирается и публикуется на GitHub Pages в рамках CI, но в
  phase-7 его функциональность не меняется.
- Единственное косвенное влияние: источник значения CWD для server-режима
  теперь — процесс запуска сервера, а не `harness.json5`. Отображение CWD в
  web-интерфейсе не требуется и остаётся вне scope phase-7.

## Если потребуется (вне scope phase-7)
- Отображать в web-интерфейсе `cwd` и `conf` из startup info.
- Показывать предупреждения валидации конфига из `GET /api/v1/config`.
