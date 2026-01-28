# Kagan - Agent Guidelines

AI-powered Kanban TUI for autonomous development workflows. Python 3.12+ with Textual framework.

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
├── database/
│   ├── models.py       # Pydantic models: Ticket, TicketCreate, TicketUpdate
│   ├── manager.py      # StateManager async database operations
│   └── schema.sql      # SQLite schema
├── mcp/                # MCP server for AI tool communication
├── sessions/           # tmux session management
├── agents/             # Planner agent + worktree management
├── styles/kagan.tcss   # ALL CSS here (no DEFAULT_CSS in Python!)
└── ui/
    ├── screens/        # kanban.py, planner.py, welcome.py
    ├── widgets/        # card.py, column.py, header.py
    └── modals/         # ticket_form.py, ticket_details.py, confirm.py
```

## Code Style

### Imports
Order: stdlib → third-party → local. Use `from __future__ import annotations`.

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
| Type | Convention | Example |
|------|------------|---------|
| Classes | PascalCase | `TicketCard`, `KanbanScreen` |
| Functions | snake_case | `get_all_tickets`, `_refresh_board` |
| Private | underscore prefix | `_get_focused_card` |
| Constants | UPPER_SNAKE | `COLUMN_ORDER`, `MIN_WIDTH` |
| Enums | PascalCase/UPPER | `TicketStatus.BACKLOG` |

### Enums (database-safe)
```python
class TicketStatus(str, Enum):
    BACKLOG = "BACKLOG"
    IN_PROGRESS = "IN_PROGRESS"

class TicketPriority(int, Enum):
    LOW = 0
    MEDIUM = 1
```

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

### Error Handling
```python
from textual.css.query import NoMatches
try:
    widget = self.query_one("#my-widget", MyWidget)
except NoMatches:
    return None
```

### Pydantic Models
```python
class Ticket(BaseModel):
    id: str = Field(default_factory=lambda: uuid4().hex[:8])
    title: str = Field(..., min_length=1, max_length=200)
    status: TicketStatus = Field(default=TicketStatus.BACKLOG)
    model_config = ConfigDict(use_enum_values=True)
```

## Testing

**Framework**: pytest with pytest-asyncio (auto mode), pytest-cov, pytest-textual-snapshot

**Test Types**:
- E2E smoke tests (`test_e2e_smoke.py`) - UI regression coverage
- Unit tests - Business logic (database, models, planner)
- Interaction tests (`test_interactions.py`) - Keyboard shortcuts

**Patterns**:
```python
# Async fixture with temp database
@pytest.fixture
async def state_manager():
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test.db"
        manager = StateManager(db_path)
        await manager.initialize()
        yield manager
        await manager.close()

# In-memory database for UI tests
@pytest.fixture
def app():
    return KaganApp(db_path=":memory:")

# UI tests with pilot
async def test_navigation(self, app: KaganApp):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.press("j")
        await pilot.pause()
        assert app.focused is not None
```

## Ruff Configuration

Line length: 100. Rules: E, F, I, UP, B, SIM, TCH, RUF.

**Always run `uv run poe fix` before manual edits** - ruff auto-fixes most issues.

Ignored rules:
- `RUF012`: Textual class attrs (BINDINGS) don't need ClassVar
- `SIM102/SIM117`: Allow nested if/with for readability

## Key Rules

1. **CSS in `.tcss` only** - All styles in `kagan.tcss`, never use `DEFAULT_CSS`
2. **Async database** - All DB operations via aiosqlite StateManager
3. **Constants module** - Use `kagan.constants` for shared values
4. **Property assertions** - Use `@property` with `assert` for required state
5. **Module size limits** - Keep modules ~150-250 LOC; test files < 200 LOC

## Git Commit Rules

Disable GPG signing in agent workflows to avoid timeouts:
```bash
git config commit.gpgsign false
```

For test fixtures:
```python
await asyncio.create_subprocess_exec(
    "git", "config", "commit.gpgsign", "false", cwd=repo_path
)
```
