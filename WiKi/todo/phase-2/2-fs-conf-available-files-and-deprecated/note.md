# Действия с вашей стороны

Корневой `harness.json5` не менялся (вы сказали сделать это самостоятельно).

Нужно:
1. `extra-scope` → `available-files`
2. добавить обязательные ключи, например:
   ```json5
   "deprecated-dirs": ["target", ".log"],
   "deprecated-files": []
   ```
3. убедиться, что нет пересечений available ∩ deprecated (иначе `harness.exe` завершится с кодом 1 до цикла).
