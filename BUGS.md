# Kagan Operational Bugs Observed During GH Initiative Orchestration

Date: 2026-02-13
Project: `kagan` (`c0cb25a3`)
Context: Creating and executing `.github/context/github-plugin-v1` GH tasks purely via Kagan MCP tools.

## 1) `tasks_create` contract/type mismatch for `acceptance_criteria` [RESOLVED]
- Observed behavior:
  - `tasks_create` rejects string input for `acceptance_criteria` with validation error:
    - "Input should be a valid list"
- Why this is anomalous:
  - Tool/interface docs in this environment advertised `acceptance_criteria` as a scalar string.
  - Runtime requires a list.
- Impact:
  - Initial bulk task creation failed; orchestration required retry with list payloads.
- Suggested fix:
  - Align tool schema/docs and runtime validation to the same type.
- Resolution:
  - `acceptance_criteria` now accepts either a single string or a list of strings in MCP and core request handlers.
  - Tool signatures and docs were updated to reflect accepted input forms.

## 2) `tasks_wait` appears non-functional (opaque empty error) [RESOLVED]
- Observed behavior:
  - Multiple calls to `tasks_wait` returned:
    - `Error executing tool tasks_wait: `
  - No code/message/hint payload.
- Inputs attempted:
  - `wait_for_status` as JSON string, CSV string, and list.
  - `timeout_seconds` as string and integer.
- Impact:
  - Could not use intended long-poll mechanism for task status progression.
  - Forced fallback to manual polling through `tasks_list`.
- Suggested fix:
  - Return typed validation and server errors with non-empty message/code.
  - Verify request decoding for `wait_for_status` and `timeout_seconds`.
- Resolution:
  - `tasks_wait` now accepts numeric-string `timeout_seconds` and `wait_for_status` as list, CSV string, or JSON-list string.
  - Bridge errors now guarantee non-empty fallback messages when core returns blank error text.

## 3) `get_task` can fail with chunk/separator errors [RESOLVED]
- Observed behavior:
  - `get_task` (summary/full) intermittently failed with:
    - "Separator is not found, and chunk exceed the limit"
    - "Separator is found, but chunk is longer than limit"
- Impact:
  - Direct task introspection became unreliable while task execution was active.
  - Required fallback to `tasks_list`/`get_context`.
- Suggested fix:
  - Harden chunking/stream framing in `get_task` response path.
  - Truncate or paginate large fields (logs/scratchpad) safely with structured metadata.
- Resolution:
  - Added truncation for large `description` and `acceptance_criteria` fields.
  - Added response-size budgeting with a final safety valve to keep payloads transport-safe.

## 4) `tasks_list(include_scratchpad=true)` did not return scratchpad content [RESOLVED]
- Observed behavior:
  - `tasks_list` with `include_scratchpad=true` still returned `scratchpad: null` for active task.
  - `get_context` for same task showed non-empty scratchpad.
- Impact:
  - Inconsistent observability between task-list and task-context APIs.
- Suggested fix:
  - Ensure `include_scratchpad` is honored consistently in `tasks_list`.
- Resolution:
  - `tasks.list` handler now reads `include_scratchpad` and populates per-task scratchpad content.
