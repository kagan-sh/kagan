---
title: MCP tools reference
description: Consolidated tool catalog, contract semantics, and scope rules
icon: material/tools
---

# MCP tools reference

This document defines the consolidated MCP toolset for Kagan.
It is a breaking, non-backward-compatible contract.

## Annotation model

| Annotation    | Meaning                           |
| ------------- | --------------------------------- |
| `read-only`   | Reads state only                  |
| `mutating`    | Modifies state                    |
| `mixed`       | Action-dependent read/write modes |
| `destructive` | Irreversible/high-impact action   |

## Tool catalog

### Core task workflow

| Tool               | Annotation    | Purpose                                                                           |
| ------------------ | ------------- | --------------------------------------------------------------------------------- |
| `task_get(...)`    | `read-only`   | Read bounded task snapshot (`summary`/`full`) or bounded context (`mode=context`) |
| `task_logs(...)`   | `read-only`   | Read paginated task execution logs (newest-first pages)                           |
| `task_list(...)`   | `read-only`   | List tasks with optional filtering and scratchpad inclusion                       |
| `task_wait(...)`   | `read-only`   | Long-poll task status changes                                                     |
| `task_create(...)` | `mutating`    | Create a task                                                                     |
| `task_patch(...)`  | `mutating`    | Apply partial task updates, transitions, and note append                          |
| `task_delete(...)` | `destructive` | Delete a task                                                                     |

### Automation jobs

| Tool              | Annotation  | Purpose                                                  |
| ----------------- | ----------- | -------------------------------------------------------- |
| `job_start(...)`  | `mutating`  | Submit async automation action for a task                |
| `job_poll(...)`   | `read-only` | Read job state; optionally wait and/or page event stream |
| `job_cancel(...)` | `mutating`  | Cancel a submitted job                                   |

### PAIR session lifecycle

| Tool                  | Annotation | Purpose                                       |
| --------------------- | ---------- | --------------------------------------------- |
| `session_manage(...)` | `mixed`    | `open`, `read`, or `close` PAIR session state |

### Project, review, and admin

| Tool                | Annotation    | Purpose                                                      |
| ------------------- | ------------- | ------------------------------------------------------------ |
| `project_list(...)` | `read-only`   | List recent projects                                         |
| `project_open(...)` | `mutating`    | Open/switch project                                          |
| `repo_list(...)`    | `read-only`   | List repos by project                                        |
| `review_apply(...)` | `destructive` | Apply review action (`approve`, `reject`, `merge`, `rebase`) |
| `audit_list(...)`   | `read-only`   | List recent audit events                                     |
| `settings_get()`    | `read-only`   | Read allowlisted settings                                    |
| `settings_set(...)` | `mutating`    | Update allowlisted settings                                  |
| `plan_submit(...)`  | `mutating`    | Submit planner proposal payload (planner profile)            |

Review semantics:

- `review_apply(action="approve")` records approval state but does **not** move task to `DONE`.
- `DONE` is reached by completion flows (for example `review_apply(action="merge")` or no-change close flow).

## `task_get` API

`task_get` supports three modes:

- `summary`: compact task payload
- `full`: expanded bounded task payload
- `context`: full bounded context (workspace + linked tasks)

### Parameters

| Parameter            | Type           | Description                               |
| -------------------- | -------------- | ----------------------------------------- |
| `task_id`            | `string`       | Target task                               |
| `mode`               | `string`       | `summary`, `full`, or `context`           |
| `include_scratchpad` | `bool \| null` | Include scratchpad (summary/full modes)   |
| `include_logs`       | `bool \| null` | Include bounded logs (summary/full modes) |
| `include_review`     | `bool \| null` | Include review payload when available     |

### Notes

- Scratchpad/log payloads are transport-bounded by default for reliability.
- When optional payloads are reduced due transport safety, response may include:
  - `scratchpad_truncated: true`
  - `logs_truncated: true`
- When `include_logs=true`, pagination hints may be included:
  - `logs_total_runs`, `logs_returned_runs`
  - `logs_has_more`, `logs_next_offset`

## `task_logs` API

Use this tool to fetch additional log history when `task_get(..., include_logs=true)` is truncated
or indicates more pages are available.

### Parameters

| Parameter | Type     | Default  | Description         |
| --------- | -------- | -------- | ------------------- |
| `task_id` | `string` | required | Target task         |
| `limit`   | `int`    | `5`      | Runs per page       |
| `offset`  | `int`    | `0`      | Newest-first offset |

### Response fields

| Field           | Type          | Description                            |
| --------------- | ------------- | -------------------------------------- |
| `logs`          | `list`        | Returned run log entries               |
| `total_runs`    | `int`         | Total runs available                   |
| `returned_runs` | `int`         | Runs returned in this response         |
| `has_more`      | `bool`        | Whether another page is available      |
| `next_offset`   | `int \| null` | Offset to fetch the next older page    |
| `truncated`     | `bool`        | Whether content was reduced for safety |

## `task_patch` API

`task_patch` is the single task mutation endpoint for incremental updates.

### Parameters

| Parameter     | Type             | Description                                                                   |
| ------------- | ---------------- | ----------------------------------------------------------------------------- |
| `task_id`     | `string`         | Target task                                                                   |
| `set`         | `object \| null` | Partial field updates (`title`, `description`, `priority`, `task_type`, etc.) |
| `transition`  | `string \| null` | `request_review`, `set_status`, or `set_task_type`                            |
| `append_note` | `string \| null` | Text appended to task notes/scratchpad                                        |

## `task_wait` long-poll API

`task_wait` blocks until task status changes or timeout is reached.

### Parameters

| Parameter         | Type              | Default               | Description             |
| ----------------- | ----------------- | --------------------- | ----------------------- |
| `task_id`         | `string`          | required              | Task to watch           |
| `timeout_seconds` | `float \| string` | server default (1800s) | Maximum wait duration  |
| `wait_for_status` | `list \| string`  | `null`                | Optional status filter  |
| `from_updated_at` | `string`          | `null`                | Race-safe resume cursor |

### Response codes

| Code                   | Meaning                            |
| ---------------------- | ---------------------------------- |
| `TASK_CHANGED`         | Task status changed                |
| `ALREADY_AT_STATUS`    | Task already matches filter        |
| `CHANGED_SINCE_CURSOR` | Task changed after supplied cursor |
| `WAIT_TIMEOUT`         | Timeout reached without status change |
| `WAIT_INTERRUPTED`     | Wait cancelled/interrupted         |
| `TASK_DELETED`         | Task deleted while waiting         |
| `INVALID_TIMEOUT`      | Invalid timeout value              |
| `INVALID_PARAMS`       | Invalid parameter payload          |

## `job_poll` API

`job_poll` consolidates job get/wait/events.

### Parameters

| Parameter         | Type     | Default  | Description                               |
| ----------------- | -------- | -------- | ----------------------------------------- |
| `job_id`          | `string` | required | Target job                                |
| `task_id`         | `string` | required | Parent task                               |
| `wait`            | `bool`   | `false`  | If true, wait for progress/terminal state |
| `timeout_seconds` | `float`  | `1.5`    | Wait timeout when `wait=true`             |
| `events`          | `bool`   | `false`  | If true, return paginated events          |
| `limit`           | `int`    | `50`     | Event page size when `events=true`        |
| `offset`          | `int`    | `0`      | Event page offset when `events=true`      |

## `session_manage` API

`session_manage` consolidates PAIR session lifecycle operations.

### Parameters

| Parameter         | Type             | Description                       |
| ----------------- | ---------------- | --------------------------------- |
| `action`          | `string`         | `open`, `read`, or `close`        |
| `task_id`         | `string`         | Target task                       |
| `reuse_if_exists` | `bool`           | Used by `open`                    |
| `worktree_path`   | `string \| null` | Optional path override for `open` |

## Scope and isolation

- Task mutations are enforced against task-scoped sessions (`task:<task_id>`).
- PAIR workers use task-scoped MCP sessions with capability lane `pair_worker`.
- AUTO workers use task-scoped MCP sessions resolved from runtime permission policy.
- Scoped task sessions cannot mutate other task IDs.
- Global MCP access does not override task-scoped worker isolation.

### Timeout configuration

Default and max timeouts are server-side configurable via settings:

- `general.tasks_wait_default_timeout_seconds`
- `general.tasks_wait_max_timeout_seconds`

## Task field semantics

- `status` is Kanban state: `BACKLOG`, `IN_PROGRESS`, `REVIEW`, `DONE`.
- `status=AUTO` and `status=PAIR` are rejected with `TASK_TYPE_VALUE_IN_STATUS`.
- Do not set `status=DONE` via generic task patch/move workflows.
  Use review completion flows to reach `DONE`.
- `task_type` is execution mode: `AUTO`, `PAIR`.
- `acceptance_criteria` accepts either a single string or a list of strings.

## Common recovery codes

| Code               | Meaning                                   | Typical action                      |
| ------------------ | ----------------------------------------- | ----------------------------------- |
| `START_PENDING`    | Job accepted, pending scheduler admission | Poll with `job_poll(wait=true)`     |
| `DISCONNECTED`     | Core unavailable                          | Start/restart core, retry           |
| `AUTH_STALE_TOKEN` | MCP token is stale after core restart     | Reconnect MCP client                |
| `WAIT_TIMEOUT`     | `task_wait` timed out without status change | Retry with same or adjusted timeout |
| `WAIT_INTERRUPTED` | `task_wait` was interrupted/cancelled     | Retry with `from_updated_at` cursor |

## Capability profiles

Higher profiles include lower-level permissions.

| Profile       | Scope                                                                 |
| ------------- | --------------------------------------------------------------------- |
| `viewer`      | Read-only operations                                                  |
| `planner`     | `viewer` + `plan_submit`                                              |
| `pair_worker` | `planner` + `task_patch`, jobs, and `session_manage`                  |
| `operator`    | `pair_worker` + `task_create`, `project_open`, non-destructive review |
| `maintainer`  | `operator` + `task_delete`, destructive review/admin operations       |

## Identity lanes

| Identity      | Notes                                         |
| ------------- | --------------------------------------------- |
| `kagan`       | Default safe lane                             |
| `kagan_admin` | Explicit elevated lane for trusted automation |
