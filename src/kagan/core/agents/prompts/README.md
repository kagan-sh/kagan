# Agent Prompt Templates

Markdown prompt templates loaded at runtime via `importlib.resources` by
`kagan.agents.prompt_loader`.

## Files

| File               | Used By                                          | Purpose                                                                                                                                                                        |
| ------------------ | ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `run_prompt.md`    | `kagan.agents.prompt.build_prompt()`             | System prompt for AUTO-mode implementation agents. Instructs the agent on task execution, git commit conventions, coordination with parallel agents, and completion signaling. |
| `review_prompt.md` | `kagan.agents.prompt_loader.get_review_prompt()` | System prompt for the AI code-review agent. Defines mandatory checks (commits exist, diff non-empty), quality criteria, and structured approve/reject output format.           |

## How Templates Are Loaded

Templates are loaded once via `importlib.resources.files("kagan.agents.prompts")`
and cached with `@functools.cache`. Context variables use Python `str.format()`
placeholders (e.g., `{task_id}`, `{title}`). See each file's HTML comment header
for the full list of expected variables.

## Adding a New Prompt

1. Add a `.md` file in this directory.
1. Load it in `prompt_loader.py` using `_load_prompt_template("filename.md")`.
1. Add an HTML comment header documenting purpose and context variables.
