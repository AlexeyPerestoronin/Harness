# Действия с вашей стороны

1. Закоммитить `.github/workflows/ci.yml` и запушить в ветку `master` (другие ветки / PR workflow не запускают).
2. В репозитории открыть Actions → workflow `CI` и дождаться зелёного прогона на `ubuntu-latest` и `windows-latest`.
3. Со страницы успешного run скачать артефакты `harness-1.0.0` и `harness-1.0.0.exe`.
4. Если вкладка Actions пустая или workflow не стартует — включить GitHub Actions в Settings репозитория.
