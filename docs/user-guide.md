# User Guide

Kagan is a keyboard-first Kanban TUI that can drive autonomous coding agents against your
repo. You stay in control: you decide when work starts, what gets reviewed, and when work
is merged.

## Prerequisites

- Python 3.12+
- `uv` installed
- A modern terminal (minimum size 80x20)
- Git repository (required for agent worktrees and review/merge)
- An ACP-compatible agent CLI on your PATH (for example, `claude`) or a configured agent

## Install and run

From the repo root:

```bash
uv run kagan
```

Optional flags:

```bash
uv run kagan --version
uv run kagan --config /path/to/.kagan/config.toml
uv run kagan --db /path/to/.kagan/state.db
```

If you pass `--config` without `--db`, Kagan stores `state.db` alongside the config file.

## First run: what gets created

Kagan stores local state under `.kagan/` in your project root:

- `.kagan/state.db` (SQLite database)
- `.kagan/config.toml` (optional config; defaults are used if missing)
- `.kagan/kagan.lock` (single-instance lock)
- `.kagan/worktrees/` (git worktrees per ticket)

## Main screen overview

The Kanban board has four columns and a header bar:

- Columns: BACKLOG, IN_PROGRESS, REVIEW, DONE
- Header: current branch, active agents, and ticket count
- Cards: title, priority icon, short ID, created date, and optional hat tag

## Keyboard controls (Kanban board)

| Key | Action |
| --- | ------ |
| `h` / `l` | Move focus left/right |
| `j` / `k` | Move focus down/up |
| `n` | New ticket |
| `e` | Edit ticket |
| `d` | Delete ticket |
| `[` / `]` | Move ticket backward/forward |
| `Enter` | View ticket details |
| `s` | Start agent on ticket |
| `x` | Stop agent |
| `o` | View agent output |
| `c` | Open planner chat |
| `Esc` | Deselect focused card |
| `?` | Command palette |
| `q` | Quit |

### Mouse

- Click a card to open details.
- Drag a card left/right to move it between columns.

## Ticket lifecycle

1. **BACKLOG**: Idea captured, not started.
2. **IN_PROGRESS**: Work active. Agents can only start here.
3. **REVIEW**: Work completed and ready for automated review/merge.
4. **DONE**: Merged to the base branch and closed.

Move tickets with `[` / `]`, or drag them between columns.

## Planner chat (create tickets from goals)

Open the planner chat with `c` and describe your goal. The planner agent generates a
structured ticket and Kagan creates it automatically.

- `Esc` returns to the Kanban board
- `Ctrl+C` interrupts a running planner

## Running agents

### Manual mode (default)

- Select a ticket in **IN_PROGRESS** and press `s`.
- Kagan runs a single agent iteration using the ticket description (and any hat prompt).
- If the agent replies with `<complete/>`, the ticket moves to **REVIEW**.
- Otherwise, the ticket stays in **IN_PROGRESS** for more work.

You can open the live output with `o` and stop the agent with `x`.

### Auto mode (scheduler)

Enable `auto_start = true` in config to let Kagan run a full loop:

- Spawn agents for **IN_PROGRESS** tickets (up to `max_concurrent_agents`).
- Run multiple iterations until `<complete/>`, `<blocked/>`, or max iterations.
- Move completed tickets to **REVIEW**.
- Run a review agent and attempt a squash-merge to the base branch.
- Mark tickets **DONE** on approval, or return them to **IN_PROGRESS** on rejection or
  merge conflict.

Auto mode requires a Git repo and a valid base branch.

## Review and merge behavior

When a ticket reaches **REVIEW** (manual or auto):

- Auto mode runs a review agent using the commit log and diff summary.
- Approved reviews are squashed and merged into the base branch.
- On merge conflicts or rejection, the ticket returns to **IN_PROGRESS**.
- In manual mode, **REVIEW** is a staging column; no automatic review runs.

## Worktrees and branches

Kagan isolates agent work in Git worktrees:

- Worktrees live at `.kagan/worktrees/<ticket-id>`
- Branch names follow `kagan/<ticket-id>-<slug>`

If you want to inspect or edit changes manually, open the ticket worktree directory.

## Permissions and safety

Agents can request permission for potentially dangerous operations. Kagan will display a
permission modal where you can allow or reject the request.

## Configuration quick start

Minimal config (optional):

```toml
[general]
max_concurrent_agents = 3
auto_start = false
max_iterations = 10
iteration_delay_seconds = 2.0
default_base_branch = "main"

[agents.claude]
identity = "anthropic.claude"
name = "Claude Code"
short_name = "claude"
protocol = "acp"
active = true

[agents.claude.run_command]
"*" = "claude"
```

Tips:

- Auto mode requires at least one configured agent. Manual mode falls back to `claude`.
- Set `default_base_branch` if your repo does not use `main`.

## Troubleshooting

- **"Another instance running"**: Close the other Kagan window. If it crashed, remove
  `.kagan/kagan.lock` and restart.
- **Agents not starting**: Ensure the configured `run_command` exists on PATH.
- **Review never runs**: Auto review requires `auto_start = true`.
- **Merge conflicts**: Kagan will abort and return the ticket to **IN_PROGRESS**.
