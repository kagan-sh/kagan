---
title: MCP Tools Reference
description: Tool catalog, annotations, and capability profiles
icon: material/tools
---

# MCP Tools Reference

## Tool annotations

- `read-only`: no state mutation
- `mutating`: changes state
- `destructive`: irreversible or high-impact

## Tool catalog

### Context and coordination

| Tool                                  | Annotation  | Purpose                                           |
| ------------------------------------- | ----------- | ------------------------------------------------- |
| `get_context(task_id)`                | `read-only` | Task details + workspace context                  |
| `get_task(task_id, ...)`              | `read-only` | Task details with optional logs/scratchpad/review |
| `update_scratchpad(task_id, content)` | `mutating`  | Append notes and progress                         |
| `request_review(task_id, summary)`    | `mutating`  | Move task to `REVIEW`                             |

### Task management

| Tool                        | Annotation    | Purpose              |
| --------------------------- | ------------- | -------------------- |
| `tasks_list(...)`           | `read-only`   | List tasks           |
| `tasks_create(...)`         | `mutating`    | Create task          |
| `tasks_update(...)`         | `mutating`    | Update task fields   |
| `tasks_move(...)`           | `mutating`    | Move status column   |
| `jobs_submit(task_id, ...)` | `mutating`    | Submit AUTO job      |
| `jobs_get(job_id, task_id)` | `read-only`   | Read job status      |
| `jobs_wait(job_id, ...)`    | `read-only`   | Wait for job status  |
| `jobs_cancel(job_id, ...)`  | `mutating`    | Cancel submitted job |
| `tasks_delete(task_id)`     | `destructive` | Delete task          |

**Key distinctions:**

- **`status`** = Kanban column (`BACKLOG`/`IN_PROGRESS`/`REVIEW`/`DONE`); **`task_type`** = execution mode (`AUTO`/`PAIR`).
- **`jobs_submit`**: requires `task_type="AUTO"`; may return `START_PENDING` before admission.
- **`tasks_create`/`tasks_update`**: auto-normalize `status="AUTO"|"PAIR"` into `task_type` (code: `STATUS_WAS_TASK_TYPE`).
- **`tasks_move`**: rejects `status="AUTO"|"PAIR"` with remediation (`next_tool="tasks_update"`).
- **`get_task(include_logs=true)`**: `mode="summary"` limits payload; `mode="full"` includes deeper history within a budget.
- **Runtime fields** on `tasks_list`/`get_task`/`get_context`: `is_running`, `is_reviewing`, `is_blocked`, `is_pending`, + detail fields.

### PAIR session control

| Tool                   | Annotation  | Purpose                   |
| ---------------------- | ----------- | ------------------------- |
| `sessions_create(...)` | `mutating`  | Create/reuse PAIR session |
| `sessions_exists(...)` | `read-only` | Check session existence   |
| `sessions_kill(...)`   | `mutating`  | Terminate session         |

### Projects and repos

| Tool                        | Annotation  | Purpose             |
| --------------------------- | ----------- | ------------------- |
| `projects_list(...)`        | `read-only` | List projects       |
| `projects_create(...)`      | `mutating`  | Create a project    |
| `projects_open(project_id)` | `mutating`  | Open/switch project |
| `repos_list(project_id)`    | `read-only` | List repos          |

### Review and planning

| Tool                           | Annotation    | Purpose                                |
| ------------------------------ | ------------- | -------------------------------------- |
| `review(task_id, action, ...)` | `destructive` | `approve`, `reject`, `merge`, `rebase` |
| `propose_plan(tasks, todos)`   | `mutating`    | Submit structured plan (planner only)  |
| `audit_tail(...)`              | `read-only`   | Read recent audit events               |

### Settings

| Tool                   | Annotation  | Purpose                     |
| ---------------------- | ----------- | --------------------------- |
| `settings_get()`       | `read-only` | Fetch allowlisted settings  |
| `settings_update(...)` | `mutating`  | Update allowlisted settings |

## Recovery-friendly responses

Mutating tools may return `code`, `hint`, `next_tool`, and `next_arguments` for deterministic recovery. Connection failures return `DISCONNECTED` or `AUTH_STALE_TOKEN`. Follow these fields directly instead of guessing a retry.

## Capability profiles

Hierarchical -- higher profiles include lower permissions.

| Profile       | Includes                                                   |
| ------------- | ---------------------------------------------------------- |
| `viewer`      | Read-only queries                                          |
| `planner`     | `viewer` + `propose_plan`                                  |
| `pair_worker` | `planner` + task progress tools                            |
| `operator`    | `pair_worker` + create/update/move + review approve/reject |
| `maintainer`  | `operator` + destructive/admin actions                     |

## Identity lanes

- `kagan`: safe default lane
- `kagan_admin`: explicit admin lane for external orchestration

## Security defaults

1. Prefer `viewer` or `pair_worker` for automation
1. Use `maintainer` only for trusted admin flows
1. Scope sessions with `--session-id` where possible
1. Use `--readonly` for reporting/inspection agents
