# Kagan

Keyboard-first Kanban TUI for AI-powered development.

## Quick start

```bash
uv run kagan
```

## Docs

- [User Guide](user-guide.md)
- [Configuration](config.md)

## Supported AI CLIs

Available now:

- Claude Code
- OpenCode

Coming soon:

- Gemini
- Codex
- More providers

## Key bindings (cheat sheet)

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
| a      | Start agent (AUTO)                       |
| w      | Watch agent (AUTO)                       |
| /      | Search                                   |
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

- **PAIR**: tmux session with an agent. Press Enter to open.
- **AUTO**: agents run autonomously. Press Enter to watch.
