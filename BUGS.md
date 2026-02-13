# Kagan Operational Bugs Observed During GH Initiative Orchestration

Date: 2026-02-13
Project: `kagan` (`c0cb25a3`)
Context: Creating and executing `.github/context/github-plugin-v1` GH tasks purely via Kagan MCP tools.
Naming note: Early observations used legacy tool names (`tasks_*`, `get_task`, `get_context`).
Current docs use consolidated names (`task_*`, `job_*`, `task_get(mode=context)`, `task_stream`).

## 1) `task_create` contract/type mismatch for `acceptance_criteria` [RESOLVED]
- Observed behavior:
  - `tasks_create`/`task_create` rejected string input for `acceptance_criteria` with validation error:
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

## 2) `task_wait` appears non-functional (opaque empty error) [RESOLVED]
- Observed behavior:
  - Multiple calls to `tasks_wait`/`task_wait` returned:
    - `Error executing tool tasks_wait: `
  - No code/message/hint payload.
- Inputs attempted:
  - `wait_for_status` as JSON string, CSV string, and list.
  - `timeout_seconds` as string and integer.
- Impact:
  - Could not use intended long-poll mechanism for task status progression.
  - Forced fallback to manual polling through `tasks_list`/`task_list`.
- Suggested fix:
  - Return typed validation and server errors with non-empty message/code.
  - Verify request decoding for `wait_for_status` and `timeout_seconds`.
- Resolution:
  - `tasks_wait`/`task_wait` now accepts numeric-string `timeout_seconds` and `wait_for_status` as list, CSV string, or JSON-list string.
  - Bridge errors now guarantee non-empty fallback messages when core returns blank error text.

## 3) `task_get` can fail with chunk/separator errors [RESOLVED]
- Observed behavior:
  - `get_task`/`task_get` (summary/full) intermittently failed with:
    - "Separator is not found, and chunk exceed the limit"
    - "Separator is found, but chunk is longer than limit"
- Impact:
  - Direct task introspection became unreliable while task execution was active.
  - Required fallback to `tasks_list`/`task_list` and `get_context`/`task_get(mode=context)`.
- Suggested fix:
  - Harden chunking/stream framing in `get_task`/`task_get` response path.
  - Truncate or paginate large fields (logs/scratchpad) safely with structured metadata.
- Resolution:
  - Added truncation for large `description` and `acceptance_criteria` fields.
  - Added response-size budgeting with a final safety valve to keep payloads transport-safe.

## 4) `task_list(include_scratchpad=true)` did not return scratchpad content [RESOLVED]
- Observed behavior:
  - `tasks_list`/`task_list` with `include_scratchpad=true` still returned `scratchpad: null` for active task.
  - `get_context`/`task_get(mode=context)` for same task showed non-empty scratchpad.
- Impact:
  - Inconsistent observability between task-list and task-context APIs.
- Suggested fix:
  - Ensure `include_scratchpad` is honored consistently in `tasks_list`/`task_list`.
- Resolution:
  - `tasks.list` handler now reads `include_scratchpad` and populates per-task scratchpad content.

## 5) `task_get(mode=full)` still exceeds MCP transport chunk limits [OPEN/REGRESSION]
- Observed behavior:
  - `get_task`/`task_get` with full payload flags can still fail with:
    - "Separator is not found, and chunk exceed the limit"
  - Reproduced with:
    - `get_task(task_id=..., mode=full, include_logs=true, include_scratchpad=true)` (legacy)
    - `task_get(task_id=..., mode=context, include_logs=true, include_scratchpad=true)` (current naming)
- Current contrasting behavior:
  - `task_wait` now returns structured timeout/status responses (no empty opaque error).
  - `task_list(include_scratchpad=true)` now returns scratchpad text.
- Impact:
  - Full task introspection remains unreliable for long-running tasks with large scratchpad/log payloads.
  - Orchestration has to use narrower reads (`mode=summary`, filtered `task_list`, and `task_stream`) as a workaround.
- Suggested fix:
  - Apply response-budgeting/safety-valve logic to the full `task_get` payload path after logs/scratchpad inclusion.
  - Consider explicit truncation metadata per large field (`logs_truncated`, `scratchpad_truncated`) and optional pagination.

## 6) MCP docs still referenced legacy tool names in setup/troubleshooting [RESOLVED]
- Observed behavior:
  - MCP setup and troubleshooting pages used old names (`tasks_list`, `jobs_submit`, `jobs_wait`, `jobs_get`).
- Impact:
  - Users could run invalid verification/recovery calls against the consolidated tool contract docs.
- Resolution:
  - Updated docs to consolidated names:
    - `task_list` for connectivity verification
    - `job_start` / `job_poll(wait=false)` for `START_PENDING` recovery
  - Updated pages:
    - `docs/guides/mcp-setup.md`
    - `docs/guides/editor-mcp-setup.md`
    - `docs/troubleshooting.md`
    - `docs/reference/mcp-tools.md`
