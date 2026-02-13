---
title: MCP tools reference
description: Tool catalog, capability profiles, and recovery semantics
icon: material/tools
---

# MCP tools reference

## Annotation model

| Annotation    | Meaning                            |
| ------------- | ---------------------------------- |
| `read-only`   | Reads state only                   |
| `mutating`    | Modifies state                     |
| `destructive` | Irreversible/high-impact operation |

## Tool catalog

### Shared coordination

| Tool                         | Annotation  | Purpose                                        |
| ---------------------------- | ----------- | ---------------------------------------------- |
| `propose_plan(tasks, todos)` | `mutating`  | Submit structured plan payload                 |
| `get_task(task_id, ...)`     | `read-only` | Read task with optional logs/review/scratchpad |
| `tasks_list(...)`            | `read-only` | List tasks                                     |
| `tasks_wait(task_id, ...)`   | `read-only` | Long-poll until task status changes or timeout |
| `projects_list(...)`         | `read-only` | List projects                                  |
| `repos_list(project_id)`     | `read-only` | List repos in project                          |
| `audit_tail(...)`            | `read-only` | Read recent audit events                       |

### Task and project operations

| Tool                                  | Annotation    | Purpose                  |
| ------------------------------------- | ------------- | ------------------------ |
| `get_context(task_id)`                | `read-only`   | Task + workspace context |
| `update_scratchpad(task_id, content)` | `mutating`    | Append task notes        |
| `tasks_create(...)`                   | `mutating`    | Create task              |
| `tasks_update(...)`                   | `mutating`    | Update task fields       |
| `tasks_move(task_id, status)`         | `mutating`    | Move Kanban status       |
| `tasks_delete(task_id)`               | `destructive` | Delete task              |
| `projects_create(...)`                | `mutating`    | Create project           |
| `projects_open(project_id)`           | `mutating`    | Open/switch project      |

### Automation jobs (AUTO)

| Tool                                | Annotation  | Purpose                               |
| ----------------------------------- | ----------- | ------------------------------------- |
| `jobs_list_actions()`               | `read-only` | List valid `jobs_submit` action names |
| `jobs_submit(task_id, action, ...)` | `mutating`  | Submit async automation job           |
| `jobs_get(job_id, task_id)`         | `read-only` | Get job status/result                 |
| `jobs_wait(job_id, task_id, ...)`   | `read-only` | Wait until terminal status or timeout |
| `jobs_events(job_id, task_id, ...)` | `read-only` | Read paginated job events             |
| `jobs_cancel(job_id, task_id)`      | `mutating`  | Cancel submitted job                  |

### PAIR sessions

| Tool                       | Annotation  | Purpose                      |
| -------------------------- | ----------- | ---------------------------- |
| `sessions_create(...)`     | `mutating`  | Create/reuse PAIR session    |
| `sessions_exists(task_id)` | `read-only` | Check PAIR session existence |
| `sessions_kill(task_id)`   | `mutating`  | Terminate PAIR session       |

### Review and settings

| Tool                               | Annotation    | Purpose                                |
| ---------------------------------- | ------------- | -------------------------------------- |
| `request_review(task_id, summary)` | `mutating`    | Move task to `REVIEW`                  |
| `review(task_id, action, ...)`     | `destructive` | `approve`, `reject`, `merge`, `rebase` |
| `settings_get()`                   | `read-only`   | Read allowlisted settings              |
| `settings_update(...)`             | `mutating`    | Update allowlisted settings            |

## `tasks_wait` long-poll API

`tasks_wait` blocks until a target task changes status or the timeout elapses.
It uses event-driven wakeup (no polling loops) for efficient orchestration.

### Parameters

| Parameter         | Type           | Default               | Description                                             |
| ----------------- | -------------- | --------------------- | ------------------------------------------------------- |
| `task_id`         | `string`       | required              | Task to watch                                           |
| `timeout_seconds` | `float`        | server default (900s) | Max wait time; capped at server max                     |
| `wait_for_status` | `list[string]` | `null` (any change)   | Target statuses to wait for (e.g. `["REVIEW", "DONE"]`) |
| `from_updated_at` | `string`       | `null`                | ISO timestamp cursor for race-safe resume               |

### Response fields

| Field             | Type     | Description                                       |
| ----------------- | -------- | ------------------------------------------------- |
| `changed`         | `bool`   | Whether the task changed before timeout           |
| `timed_out`       | `bool`   | Whether the wait timed out                        |
| `task_id`         | `string` | ID of the watched task                            |
| `previous_status` | `string` | Status when wait started                          |
| `current_status`  | `string` | Status when wait ended                            |
| `changed_at`      | `string` | ISO timestamp of the change                       |
| `task`            | `object` | Compact task snapshot (no large logs/scratchpads) |
| `code`            | `string` | Machine-readable result code                      |

### Response codes

| Code                   | Meaning                                                          |
| ---------------------- | ---------------------------------------------------------------- |
| `TASK_CHANGED`         | Status changed during wait                                       |
| `ALREADY_AT_STATUS`    | Task was already at a target status (immediate return)           |
| `CHANGED_SINCE_CURSOR` | Task changed since `from_updated_at` cursor (race-safe catch-up) |
| `WAIT_TIMEOUT`         | No change detected within timeout                                |
| `WAIT_INTERRUPTED`     | Wait was cancelled/interrupted                                   |
| `TASK_DELETED`         | Task was deleted during wait                                     |
| `INVALID_TIMEOUT`      | Invalid timeout value                                            |

### Timeout configuration

Default and max timeouts are server-side configurable via settings:

- `general.tasks_wait_default_timeout_seconds` (default: 900)
- `general.tasks_wait_max_timeout_seconds` (default: 900)

### Worktree base-ref strategy

`settings_update` can also set `general.worktree_base_ref_strategy`:

- `remote` (default): prefer `origin/<base_branch>` when present
- `local_if_ahead`: use local `<base_branch>` only when it is ahead of `origin/<base_branch>`
- `local`: always prefer local `<base_branch>`

### Orchestration pattern

```
# Wait for task to reach REVIEW or DONE
result = tasks_wait(task_id="T-1", wait_for_status=["REVIEW", "DONE"])

# Race-safe resume after reconnect
result = tasks_wait(task_id="T-1", from_updated_at=last_known_updated_at)

# Short poll with timeout
result = tasks_wait(task_id="T-1", timeout_seconds=30)
if result.timed_out:
    # retry or take action
```

## Task field semantics

- `status` is Kanban state: `BACKLOG`, `IN_PROGRESS`, `REVIEW`, `DONE`.
- `task_type` is execution mode: `AUTO`, `PAIR`.

Recovery behavior:

- `tasks_move(status="AUTO"|"PAIR")` returns recovery metadata pointing to `tasks_update(..., task_type=...)`.
- `jobs_submit` requires `task_type="AUTO"`.

## Common recovery codes

| Code                   | Meaning                                         | Typical action                      |
| ---------------------- | ----------------------------------------------- | ----------------------------------- |
| `START_PENDING`        | Job accepted, pending scheduler admission       | Poll `jobs_wait` or `jobs_get`      |
| `DISCONNECTED`         | Core unavailable                                | Start/restart core, retry           |
| `AUTH_STALE_TOKEN`     | MCP token is stale after core restart           | Reconnect MCP client                |
| `STATUS_WAS_TASK_TYPE` | `status` was used where `task_type` is required | Retry with `tasks_update`           |
| `WAIT_TIMEOUT`         | `tasks_wait` timed out without a change         | Retry with same or adjusted timeout |
| `WAIT_INTERRUPTED`     | `tasks_wait` was interrupted/cancelled          | Retry with `from_updated_at` cursor |

## Capability profiles

Higher profiles include lower-level permissions.

| Profile       | Scope                                                                  |
| ------------- | ---------------------------------------------------------------------- |
| `viewer`      | Read-only operations                                                   |
| `planner`     | `viewer` + planning surface                                            |
| `pair_worker` | `planner` + task progress tools                                        |
| `operator`    | `pair_worker` + create/update/move + non-destructive review operations |
| `maintainer`  | `operator` + destructive/admin operations                              |

## Identity lanes

| Identity      | Notes                                         |
| ------------- | --------------------------------------------- |
| `kagan`       | Default safe lane                             |
| `kagan_admin` | Explicit elevated lane for trusted automation |
