# User Guide

Kagan is a keyboard-first Kanban TUI for AI-assisted development.

## Requirements

- Python 3.12+
- `uv`
- Terminal (min 80x20)
- Git repo (worktrees + review)
- tmux (PAIR mode)
- ACP-compatible agent CLI (Claude Code or OpenCode)

## Supported AI CLIs

- Claude Code
- OpenCode

## Install & run

```bash
uv run kagan
```

Common flags:

```bash
uv run kagan --version
uv run kagan --config /path/to/.kagan/config.toml
uv run kagan --db /path/to/.kagan/state.db
uv run kagan --skip-preflight
```

## First run

Created in your repo:

- `.kagan/state.db`
- `.kagan/config.toml`
- `.kagan/kagan.lock`
- `.kagan/worktrees/`

## Board layout

- Columns: BACKLOG, IN_PROGRESS, REVIEW, DONE
- Header: version, branch, sessions, ticket count
- Footer: context keybindings
- Cards: mode badge, title, priority, ID

## Keybindings

### Navigation

| Key       | Action           |
| --------- | ---------------- |
| h / Left  | Move focus left  |
| l / Right | Move focus right |
| j / Down  | Move focus down  |
| k / Up    | Move focus up    |

### Tickets

| Key    | Action                                   |
| ------ | ---------------------------------------- |
| n      | New ticket                               |
| v      | View details                             |
| e      | Edit ticket                              |
| Enter  | Open session (PAIR) / watch agent (AUTO) |
| /      | Search                                   |
| a      | Start agent (AUTO)                       |
| w      | Watch agent (AUTO)                       |
| Ctrl+D | Delete ticket                            |

### Leader (press g, then)

| Sequence | Action            |
| -------- | ----------------- |
| g h      | Move ticket left  |
| g l      | Move ticket right |
| g d      | View diff         |
| g r      | Review            |
| g w      | Watch agent       |

### Review

| Key    | Action       |
| ------ | ------------ |
| D      | View diff    |
| r      | Review modal |
| Ctrl+M | Merge ticket |

### Global

| Key        | Action           |
| ---------- | ---------------- |
| p          | Planner mode     |
| ? / Ctrl+P | Command palette  |
| Ctrl+,     | Settings         |
| Esc        | Deselect / close |
| q          | Quit             |

## Modes

- **PAIR**: tmux session with an agent. Enter opens the worktree session.
- **AUTO**: agents run autonomously. Enter (or `w`) opens live output.

## Ticket lifecycle

BACKLOG → IN_PROGRESS → REVIEW → DONE

Move tickets with `g h` / `g l`.

## Planner

Press `p`, describe a goal, approve or refine the proposed tickets.

## Review flow

1. `r` to open review
1. Inspect commits/diff
1. `a` approve / `r` reject

## Worktrees

- `.kagan/worktrees/<ticket-id>`
- Branch: `kagan/<ticket-id>-<slug>`

## MCP tools

| Tool                                    | Purpose                                                                              |
| --------------------------------------- | ------------------------------------------------------------------------------------ |
| `get_context(ticket_id)`                | Get ticket context including title, description, acceptance criteria, and scratchpad |
| `update_scratchpad(ticket_id, content)` | Append notes to ticket scratchpad                                                    |
| `request_review(ticket_id, summary)`    | Move PAIR ticket to REVIEW (requires all changes to be committed)                    |

Run server:

```bash
kagan mcp
```

## Troubleshooting

- Another instance: remove `.kagan/kagan.lock`
- Agent not found: check configured commands on PATH
- tmux missing: install tmux
- Merge conflicts: ticket returns to IN_PROGRESS
- AUTO idle: enable `auto_start` or press `a`
- Windows: use WSL2
