# Kagan Bug Validation (Post-Reinstall)

Date: 2026-02-13
Install baseline: `uv tool install . --force --reinstall`
Validation scope: all previously recorded anomalies.

## Confirmed Reproducible

## 1) `task_wait` exceeds MCP transport deadline

- Repro:
  - `task_wait(task_id="b402fd9c", wait_for_status="DONE", timeout_seconds="900")`
  - `task_wait(task_id="b402fd9c", wait_for_status="DONE", timeout_seconds="30")`
- Result:
  - Both fail with transport timeout: `timed out awaiting tools/call after 60s`.
- Root cause:
  - Server-side wait is not returning within MCP tool-call deadline.
- Proposed fix:
  - Chunk long waits into bounded server-side windows and return continuation metadata.
  - Enforce/document a safe max timeout per MCP call.

## 2) `audit_list` can overflow on larger windows

- Repro:
  - `audit_list(capability="tasks", limit=80)`
- Result:
  - `DISCONNECTED`: `Separator is not found, and chunk exceed the limit`.
- Control:
  - `audit_list(..., limit=20)` succeeded during this validation pass.
- Root cause:
  - Response budgeting/pagination is insufficient for larger payloads.
- Proposed fix:
  - Hard-cap payload size per response page and return explicit pagination/truncation metadata.

## 3) TUI merge failure path still throws uncaught backend exception

- Repro:
  - Direct backend call (`ctx.api.merge_task_direct(task)`) for `bb6fadf8` raised:
  - `RuntimeError: git push origin kagan/246862b7 ... (non-fast-forward)`.
- Root cause:
  - Pre-merge push in `src/kagan/core/services/merges.py` is outside protected error conversion.
  - TUI merge action path in `src/kagan/tui/ui/screens/kanban/review_controller.py` does not wrap merge call with defensive exception handling.
- Proposed fix:
  - Convert pre-merge push failures to structured merge failure results.
  - Add `try/except` in TUI merge action and always show explicit error toast.

## Not Currently Reproducible (Likely False Positives/Already Resolved)

## A) Task lifecycle diverges from completion evidence

- Current validation did not find non-terminal tasks carrying `<complete/>` completion evidence.
- `stop_agent` on reviewed/completed validation task returned `NOT_RUNNING` and did not regress task state.

## B) Stale/orphan task sessions remain alive

- No ACTIVE sessions found in DB after validation runs.
- No `execution_processes` left in `RUNNING` state after task completion.
- One maintainer MCP process (`ext:orchestrator`) remained active as expected for current tool session.

## C) Worker emits invalid `exclude_task_ids` shape

- Historical evidence exists in run logs (`exclude_task_ids` sent as raw string).
- Fresh runs in this install (`b9df288a`, `c06b00b0`, `975e23f0`) sent list shape correctly:
  - `exclude_task_ids: ["<task_id>"]`
- Codebase also contains guard fix commit:
  - `d801e17 fix: reject raw string exclude_task_ids in task_list args`

## D) RUNNING execution without heartbeat progress

- Not reproduced in this validation pass.
- Active run log chunks advanced continuously while running and terminalized correctly.
