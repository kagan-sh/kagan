# Kagan - Agent Guidelines

AI-powered Kanban TUI for autonomous development workflows. Python 3.12+ with Textual framework.

## MANDATORY: Internal Documentation

**BEFORE modifying any code, you MUST read the relevant internal documentation:**

| Action                                                            | Required Reading                                                 |
| ----------------------------------------------------------------- | ---------------------------------------------------------------- |
| Modify/create UI code (`ui/`, `app.py`, widgets, screens, modals) | [docs/internal/textual_rules.md](docs/internal/textual_rules.md) |
| Modify/create any test file                                       | [docs/internal/testing_rules.md](docs/internal/testing_rules.md) |
| Modify/create CSS styles                                          | [docs/internal/textual_rules.md](docs/internal/textual_rules.md) |

**Failure to read these docs before making changes will result in incorrect patterns and failed reviews.**

## Build & Development Commands

```bash
# Run application
uv run kagan                    # Production mode
uv run poe dev                  # Dev mode with hot reload

# Testing
uv run pytest tests/ -v                                    # All tests
uv run pytest tests/test_database.py -v                    # Single file
uv run pytest tests/test_database.py::TestTicketCRUD -v    # Single class
uv run pytest tests/test_database.py::TestTicketCRUD::test_create_ticket -v  # Single test
uv run pytest -m unit                                      # By marker (unit/integration/e2e/snapshot)

# Linting & Formatting
uv run poe fix                  # Auto-fix + format (run this first!)
uv run poe lint                 # Run ruff linter
uv run poe format               # Format with ruff
uv run poe typecheck            # Run pyrefly type checker
uv run poe check                # lint + typecheck + test

# Snapshot tests
UPDATE_SNAPSHOTS=1 uv run pytest tests/test_snapshots.py --snapshot-update
```

## Project Structure

```
src/kagan/
├── app.py              # Main KaganApp class
├── constants.py        # COLUMN_ORDER, STATUS_LABELS, PRIORITY_LABELS
├── config.py           # Configuration models
├── database/           # models.py (Pydantic), manager.py (async StateManager)
├── keybindings/        # Centralized keybinding registry (see below)
├── mcp/                # MCP server for AI tool communication
├── sessions/           # tmux session management
├── agents/             # Planner agent + worktree management
├── acp/                # Agent Control Protocol
├── styles/kagan.tcss   # ALL CSS here (no DEFAULT_CSS in Python!)
└── ui/
    ├── screens/        # kanban/, planner.py, welcome.py, approval.py
    ├── widgets/        # card.py, column.py, header.py, search_bar.py
    └── modals/         # ticket_details/, review.py, confirm.py, help.py
```

## Code Style

### Imports

Order: stdlib -> third-party -> local. Always use `from __future__ import annotations`.

```python
from __future__ import annotations
from typing import TYPE_CHECKING, cast
from kagan.constants import COLUMN_ORDER

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

### Enums (database-safe)

```python
class TicketStatus(str, Enum):
    BACKLOG = "BACKLOG"


class TicketPriority(int, Enum):
    LOW = 0
```

### Pydantic Models

```python
class Ticket(BaseModel):
    id: str = Field(default_factory=lambda: uuid4().hex[:8])
    title: str = Field(..., min_length=1, max_length=200)
    model_config = ConfigDict(use_enum_values=True)
```

## Testing

**IMPORTANT**: Before writing or modifying ANY test, read [docs/internal/testing_rules.md](docs/internal/testing_rules.md).

**Framework**: pytest with pytest-asyncio (auto mode), pytest-cov, pytest-textual-snapshot

| Category    | Marker                    | Mocking Allowed        |
| ----------- | ------------------------- | ---------------------- |
| unit        | `pytest.mark.unit`        | None (pure logic)      |
| integration | `pytest.mark.integration` | External services only |
| e2e         | `pytest.mark.e2e`         | Network calls only     |
| snapshot    | `pytest.mark.snapshot`    | None                   |

### Test Fixtures (from conftest.py)

`state_manager`, `git_repo`, `mock_agent`, `mock_worktree_manager`, `mock_session_manager`, `e2e_project`, `e2e_app`

```python
async def test_navigation(self, e2e_app: KaganApp):
    async with e2e_app.run_test(size=(120, 40)) as pilot:
        await pilot.press("j")
        await pilot.pause()
```

### Test Rules

- E2E tests mock at boundaries (network), not internals
- Avoid tautological tests (mock A, assert A)
- Use `@pytest.mark.parametrize` to reduce duplication
- Test files < 200 LOC; check conftest.py before creating fixtures

## Ruff Configuration

Line length: 100. Target: Python 3.12. Rules: E, F, I, UP, B, SIM, TCH, RUF.
**Always run `uv run poe fix` before manual edits** - ruff auto-fixes most issues.
Ignored: `RUF012` (Textual class attrs), `SIM102/SIM117` (nested if/with allowed).

## Key Rules

1. **CSS in `.tcss` only** - All styles in `kagan.tcss`, never use `DEFAULT_CSS`
1. **Async database** - All DB operations via aiosqlite StateManager
1. **Constants module** - Use `kagan.constants` for shared values
1. **Property assertions** - Use `@property` with `assert` for required state
1. **Module size limits** - Keep modules ~150-250 LOC; test files < 200 LOC
1. **Keybindings in registry only** - All bindings defined in `kagan.keybindings`

## Agent Roles and Capabilities

Different agent contexts have different capability profiles controlled by the `read_only` parameter:

| Agent Context    | Location           | read_only | Write Files | Terminal | Purpose                           |
| ---------------- | ------------------ | --------- | ----------- | -------- | --------------------------------- |
| Planner          | `planner.py:115`   | Yes       | No          | No       | Create tickets from natural lang  |
| Refiner          | `refiner.py:66`    | Yes       | No          | No       | Enhance prompts before submission |
| Review Modal     | `review.py:132`    | Yes       | No          | No       | AI-powered code review            |
| Scheduler Review | `scheduler.py:269` | Yes       | No          | No       | Automated ticket review           |
| Worker           | `scheduler.py:191` | No        | Yes         | Yes      | Execute ticket implementation     |

### Setting Agent Capabilities

```python
from kagan.acp.agent import Agent

# Read-only agent (planner, refiner, reviewer)
agent = Agent(project_root, config, read_only=True)

# Full capability agent (worker)
agent = Agent(project_root, config)  # read_only=False by default
```

### Why Read-Only Mode?

Read-only mode enforces **capability-based access control** at the ACP protocol level:

1. **Hard enforcement**: Agents cannot write files or run commands, regardless of prompts
1. **Defense in depth**: RPC handlers also reject operations if `read_only=True`
1. **Prevents prompt injection**: Even if an agent is tricked, it lacks the capability to act

This follows the principle: "Give agents the minimal capabilities required for their job."

## Keybindings Registry

**All keybindings are defined in `src/kagan/keybindings/`** - this is the single source of truth.

```
keybindings/
├── __init__.py     # Public API exports
├── registry.py     # KeyBindingDef dataclass + utility functions
├── app.py          # App-level bindings (quit, help, command palette)
├── kanban.py       # KanbanScreen bindings + leader key sequences
├── modals.py       # All modal bindings
├── screens.py      # Non-kanban screen bindings
└── widgets.py      # Widget-specific bindings
```

### Adding/Modifying Keybindings

1. **Define in registry** - Add `KeyBindingDef` to the appropriate file
1. **Use in component** - Import and call `to_textual_bindings()`
1. **Help modal updates automatically** - Uses `get_key_for_action()` lookups

```python
# In keybindings/kanban.py
KeyBindingDef(
    "n",  # Key
    "new_ticket",  # Action name
    "New",  # Footer label
    "primary",  # Category
    help_text="Create new ticket",
)

# In screens/kanban.py
from kagan.keybindings import KANBAN_BINDINGS, to_textual_bindings


class KanbanScreen(Screen):
    BINDINGS = to_textual_bindings(KANBAN_BINDINGS)
```

### Key Categories

| Category   | Purpose                              |
| ---------- | ------------------------------------ |
| navigation | Movement keys (h/j/k/l, arrows, tab) |
| primary    | Main actions (n, e, v, Enter, etc.)  |
| leader     | g+key sequences                      |
| context    | Context-specific (REVIEW only, etc.) |
| global     | App-wide (quit, help)                |
| modal      | Modal-specific bindings              |
| utility    | Internal (escape, ctrl+c)            |

### Terminal-Safe Keys

Avoid these conflicts:

- `ctrl+d` → Use `x` (Unix EOF)
- `ctrl+m` → Use `m` (terminal Enter)
- `ctrl+,` → Use `,` (cross-platform issues)

For test fixtures:

```python
await asyncio.create_subprocess_exec("git", "config", "commit.gpgsign", "false", cwd=repo_path)
```
