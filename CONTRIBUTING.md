# Contributing to Kagan

Thanks for your interest in contributing! This document is the canonical guide for
developers working on the codebase. User documentation lives in `docs/`.

## Prerequisites

- Python 3.12+
- `uv` for dependency management
- A terminal that supports Textual (for running the TUI)
- tmux (for PAIR mode testing)
- Git (for worktree functionality)

## Getting Started

Clone the repo and execute:

```bash
uv run kagan
```

## Development Mode

```bash
uv run poe dev
```

This runs with hot reload enabled for faster iteration.

## Linting, Formatting, Typecheck, Tests

```bash
uv run poe fix        # Auto-fix lint issues + format (run this first!)
uv run poe lint       # Ruff linter
uv run poe format     # Format with ruff
uv run poe typecheck  # Pyrefly type checker
uv run pytest tests/ -v
```

Run the full suite:

```bash
uv run poe check      # lint + typecheck + test
```

## Testing

```bash
# All tests
uv run pytest tests/ -v

# Single file
uv run pytest tests/test_database.py -v

# Single class
uv run pytest tests/test_database.py::TestTicketCRUD -v

# Single test
uv run pytest tests/test_database.py::TestTicketCRUD::test_create_ticket -v
```

## UI Snapshots

```bash
UPDATE_SNAPSHOTS=1 uv run pytest tests/test_snapshots.py --snapshot-update
```

## Docs Preview

```bash
uv run mkdocs serve
```

Open `http://127.0.0.1:8000/` in your browser.

## Project Structure

```
src/kagan/
├── app.py              # Main KaganApp class
├── constants.py        # COLUMN_ORDER, STATUS_LABELS, PRIORITY_LABELS
├── config.py           # Configuration models (Pydantic)
├── limits.py           # Timeouts and buffer limits
├── theme.py            # Custom Textual theme
├── lock.py             # Instance lock (single instance)
├── git_utils.py        # Git helper functions
├── jsonrpc.py          # JSON-RPC implementation
├── cli/                # CLI commands
│   └── update.py       # Update command
├── ansi/               # ANSI escape code handling
│   └── cleaner.py      # ANSI code cleaning utilities
├── database/
│   ├── models.py       # Pydantic models: Ticket, TicketCreate, TicketUpdate
│   ├── manager.py      # StateManager async database operations
│   └── queries.py      # SQL query helpers
├── keybindings/        # Centralized keybinding registry
│   ├── registry.py     # KeyBindingDef dataclass + utility functions
│   ├── app.py          # App-level bindings (quit, help, command palette)
│   ├── kanban.py       # KanbanScreen bindings + leader key sequences
│   ├── modals.py       # All modal bindings
│   ├── screens.py      # Non-kanban screen bindings
│   └── widgets.py      # Widget-specific bindings
├── mcp/                # Model Context Protocol server
│   ├── server.py       # FastMCP server setup
│   └── tools.py        # MCP tool implementations
├── sessions/           # tmux session management
│   ├── manager.py      # SessionManager class
│   └── tmux.py         # tmux command helpers
├── agents/             # Planner agent + scheduler
│   ├── planner.py      # Planner prompt + XML parsing
│   ├── refiner.py      # Prompt refinement agent
│   ├── refinement_rules.py  # Refinement rules
│   ├── scheduler.py    # AUTO mode ticket-to-agent scheduler
│   ├── worktree.py     # Git worktree management
│   ├── signals.py      # Agent completion signals parser
│   ├── prompt.py       # Prompt building for AUTO mode
│   ├── prompt_loader.py    # Template loading for prompts
│   └── config_resolver.py  # Agent config resolution
├── acp/                # Agent Control Protocol
│   ├── agent.py        # ACP Agent class (JSON-RPC over subprocess)
│   ├── api.py          # ACP API methods
│   ├── protocol.py     # ACP protocol types
│   ├── rpc.py          # RPC endpoint handlers
│   ├── messages.py     # Textual messages for agent events
│   ├── terminals.py    # Terminal management for agents
│   ├── terminal.py     # Single terminal handling
│   └── buffers.py      # Response buffering
├── data/
│   └── builtin_agents.py   # Built-in agent definitions (Claude, OpenCode)
├── styles/
│   └── kagan.tcss      # ALL CSS here (no DEFAULT_CSS in Python!)
└── ui/
    ├── utils/              # UI utilities
    │   └── clipboard.py    # Clipboard operations
    ├── screens/
    │   ├── base.py         # KaganScreen base class
    │   ├── kanban/         # Main Kanban board
    │   │   ├── screen.py   # KanbanScreen implementation
    │   │   ├── focus.py    # Focus management helpers
    │   │   └── actions.py  # Ticket action handlers
    │   ├── planner.py      # Planner screen (chat-first)
    │   ├── welcome.py      # First-boot setup screen
    │   ├── approval.py     # Ticket approval screen
    │   ├── ticket_editor.py    # Ticket editor screen
    │   └── troubleshooting.py  # Pre-flight check failures
    ├── widgets/
    │   ├── card.py         # TicketCard widget
    │   ├── column.py       # KanbanColumn widget
    │   ├── header.py       # KaganHeader widget
    │   ├── status_bar.py   # StatusBar widget
    │   ├── search_bar.py   # SearchBar widget
    │   ├── empty_state.py  # EmptyState widget
    │   ├── streaming_output.py  # StreamingOutput widget
    │   ├── plan_display.py     # Plan display widget
    │   ├── agent_content.py    # Agent content display
    │   ├── tool_call.py        # Tool call display
    │   └── permission_prompt.py # Permission prompt widget
    └── modals/
        ├── ticket_details/     # Unified ticket view/edit/create modal
        │   ├── modal.py
        │   └── form.py
        ├── review.py           # Review modal with AI review
        ├── settings.py         # Settings modal
        ├── confirm.py          # Confirmation dialog
        ├── diff.py             # Diff viewer modal
        ├── agent_output.py     # Agent output viewer
        ├── rejection_input.py  # Rejection feedback modal
        ├── description_editor.py   # Full-screen description editor
        ├── help.py             # Help modal
        ├── tmux_gateway.py     # Tmux gateway info modal
        ├── duplicate_ticket.py # Duplicate ticket modal
        └── actions.py          # Modal action enums
```

## Code Style

### Imports

Order: stdlib, third-party, local. Use `from __future__ import annotations`.

```python
from __future__ import annotations
from typing import TYPE_CHECKING, cast
from textual.app import ComposeResult
from kagan.constants import COLUMN_ORDER
from kagan.database.models import Ticket

if TYPE_CHECKING:
    from kagan.app import KaganApp
```

### Type Annotations

- Always annotate function signatures and class attributes
- Use `X | None` union syntax (not `Optional[X]`)
- Use `TYPE_CHECKING` block for type-only imports
- Use `cast()` for type narrowing: `return cast("KaganApp", self.app)`

### Naming Conventions

| Type      | Convention        | Example                             |
| --------- | ----------------- | ----------------------------------- |
| Classes   | PascalCase        | `TicketCard`, `KanbanScreen`        |
| Functions | snake_case        | `get_all_tickets`, `_refresh_board` |
| Private   | underscore prefix | `_get_focused_card`                 |
| Constants | UPPER_SNAKE       | `COLUMN_ORDER`, `MIN_WIDTH`         |
| Enums     | PascalCase/UPPER  | `TicketStatus.BACKLOG`              |

### Textual Patterns

```python
# Messages as dataclasses
@dataclass
class Selected(Message):
    ticket: Ticket


# Button handlers with @on decorator
@on(Button.Pressed, "#save-btn")
def on_save(self) -> None:
    self.action_submit()


# Reactive with recompose
tickets: reactive[list[Ticket]] = reactive(list, recompose=True)


# Widget IDs in __init__
def __init__(self, ticket: Ticket, **kwargs) -> None:
    super().__init__(id=f"card-{ticket.id}", **kwargs)
```

### CSS in TCSS Only

All styles go in `src/kagan/styles/kagan.tcss`. Never use `DEFAULT_CSS` in Python.

## Git Commit Rules

Disable GPG signing in agent workflows to avoid timeouts:

```bash
git config commit.gpgsign false
```

## Key Rules

1. **CSS in `.tcss` only** - All styles in `kagan.tcss`, never use `DEFAULT_CSS`
1. **Async database** - All DB operations via aiosqlite StateManager
1. **Constants module** - Use `kagan.constants` for shared values
1. **Property assertions** - Use `@property` with `assert` for required state
1. **Module size limits** - Keep modules ~150-250 LOC; test files < 200 LOC

## Notes

- Kagan uses Textual; styles should live in `src/kagan/styles/kagan.tcss`
- See `AGENTS.md` for agent workflow and coding guidelines
