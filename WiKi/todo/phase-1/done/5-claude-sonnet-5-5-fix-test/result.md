# claude_sonnet_5_5

`claude_tools` слал `input_schema: {}` для no-arg tools (`list_available_file_extension` = tools[13]) → Claude HTTP 400 `tools.13.custom.input_schema.type: Field required`.

`claude_tools` нормализует схему через `claude_input_schema`: если нет `type`, ставит `"object"` (почему: Anthropic Messages API требует JSON Schema с `type`; Grok/Gemini пустой `{}` принимают).

Live `claude_sonnet_5_5` — ok (5 итераций, файл откатили). Ввод API-ключа со stdin возвращён.
