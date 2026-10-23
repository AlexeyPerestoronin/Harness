# cli.rs
было: xor `--prompt` / `--dump` / `--conf-new`; `Start { prompt, dump }`
стало: xor `--prompt` / `--prompt-file` / `--dump` / `--conf-new`; `Start { prompt, prompt_file, dump }`; `read_prompt_file` (UTF-8, BOM, без trim)
почему: длинный промпт из файла, чтение после parse, тесты рядом с парсером

# main.rs
было: `Start { conf, prompt, dump }` сразу в сессию
стало: если `prompt_file` — прочитать файл и передать текст как `prompt: Some(text)`
почему: тот же путь, что `--prompt`; файл не читается в `parse_args`

# help.txt, README.md, README.ru.md
было: в USAGE/правилах нет `--prompt-file`
стало: USAGE, FLAGS и xor включают `--prompt-file`
почему: справка должна совпадать с CLI
