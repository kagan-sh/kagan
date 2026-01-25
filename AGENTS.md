# Kagan - Agent Guidelines

AI-powered Kanban TUI for autonomous development workflows, built with Python 3.12+ and Textual.

## Build & Development Commands

```bash
# Run the application
uv run kagan                         # Production mode
uv run poe dev                       # Dev mode with hot reload

# Testing
uv run pytest tests/ -v              # Run all tests
uv run pytest tests/test_database.py -v                    # Run single test file
uv run pytest tests/test_database.py::TestTicketCRUD -v    # Run single test class
uv run pytest tests/test_database.py::TestTicketCRUD::test_create_ticket -v  # Single test

# Linting & Formatting
uv run poe lint                      # Run ruff linter
uv run poe format                    # Format with ruff
uv run poe fix                       # Auto-fix lint issues + format
uv run poe typecheck                 # Run pyrefly type checker
uv run poe check                     # Run lint + typecheck + test

# Snapshots (for UI tests)
UPDATE_SNAPSHOTS=1 uv run pytest tests/test_snapshots.py --snapshot-update
```

## Project Structure

```
src/kagan/
├── app.py              # Main KaganApp class
├── constants.py        # COLUMN_ORDER, STATUS_LABELS, PRIORITY_LABELS, paths
├── config.py           # Configuration models (for Phase 2 agent configs)
├── database/
│   ├── models.py       # Pydantic models: Ticket, TicketCreate, TicketUpdate
│   ├── manager.py      # StateManager async database operations
│   └── schema.sql      # SQLite schema
├── styles/
│   └── kagan.tcss      # ALL CSS consolidated here (do not add DEFAULT_CSS to Python)
└── ui/
    ├── screens/kanban.py    # Main Kanban board screen
    ├── widgets/             # card.py, column.py, header.py
    └── modals/              # ticket_form.py, ticket_details.py, confirm.py, actions.py
```

## Code Style Guidelines

### Imports

Order imports by: stdlib, third-party, local. Use `from __future__ import annotations` for forward refs.

```python
from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual.app import ComposeResult
from textual.screen import Screen

from kagan.constants import COLUMN_ORDER
from kagan.database.models import Ticket

if TYPE_CHECKING:
    from kagan.app import KaganApp
```

### Type Annotations

- Always annotate function signatures and class attributes
- Use `X | None` union syntax (not `Optional[X]`)
- Use `TYPE_CHECKING` block for imports only needed for type hints
- Use `cast()` when type narrowing is needed

```python
def get_ticket(self, ticket_id: str) -> Ticket | None: ...

@property
def kagan_app(self) -> KaganApp:
    return cast("KaganApp", self.app)
```

### Textual Patterns

**Messages as dataclasses:**
```python
from dataclasses import dataclass
from textual.message import Message

@dataclass
class Selected(Message):
    ticket: Ticket
```

**Use `@on` decorator for button handlers:**
```python
from textual import on

@on(Button.Pressed, "#save-btn")
def on_save(self) -> None:
    self.action_submit()
```

**Use `reactive` with `recompose=True`:**
```python
from textual.reactive import reactive

tickets: reactive[list[Ticket]] = reactive(list, recompose=True)
```

**Widget IDs in `__init__`:**
```python
def __init__(self, ticket: Ticket, **kwargs) -> None:
    super().__init__(id=f"card-{ticket.id}", **kwargs)
```

**CSS in `.tcss` files only:**
- All styles go in `src/kagan/styles/kagan.tcss`
- Do NOT use `DEFAULT_CSS` class attribute in Python files

**Property-based state access:**
```python
@property
def state_manager(self) -> StateManager:
    assert self._state_manager is not None
    return self._state_manager
```

### Naming Conventions

| Type | Convention | Example |
|------|------------|---------|
| Classes | PascalCase | `TicketCard`, `KanbanScreen` |
| Functions/methods | snake_case | `get_all_tickets`, `_refresh_board` |
| Private methods | leading underscore | `_get_focused_card` |
| Constants | UPPER_SNAKE | `COLUMN_ORDER`, `MIN_WIDTH` |
| Enums | PascalCase class, UPPER values | `TicketStatus.BACKLOG` |

### Enums

Use string/int enums for database-safe values:

```python
class TicketStatus(str, Enum):
    BACKLOG = "BACKLOG"
    IN_PROGRESS = "IN_PROGRESS"

class TicketPriority(int, Enum):
    LOW = 0
    MEDIUM = 1
    HIGH = 2
```

### Error Handling

- Use specific exceptions, not bare `except:`
- Import and use `NoMatches` from Textual for query failures

```python
from textual.css.query import NoMatches

try:
    widget = self.query_one("#my-widget", MyWidget)
except NoMatches:
    return None
```

### Testing Patterns

**Async fixtures with temporary database:**
```python
@pytest.fixture
async def state_manager():
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test.db"
        manager = StateManager(db_path)
        await manager.initialize()
        yield manager
        await manager.close()
```

**UI tests with pilot:**
```python
async def test_navigation(self, app: KaganApp):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.press("j")  # Navigate down
        await pilot.pause()
        assert app.focused is not None
```

**In-memory database for UI tests:**
```python
@pytest.fixture
def app():
    return KaganApp(db_path=":memory:")
```

### Pydantic Models

Use Field defaults and validation:

```python
class Ticket(BaseModel):
    id: str = Field(default_factory=lambda: uuid4().hex[:8])
    title: str = Field(..., min_length=1, max_length=200)
    status: TicketStatus = Field(default=TicketStatus.BACKLOG)

    model_config = ConfigDict(use_enum_values=True)
```

### Ruff Configuration

Line length: 100 chars. Key rules enabled: E, F, I, UP, B, SIM, TCH, RUF.

Ignored:
- `RUF012`: Textual class attributes (BINDINGS, etc.) don't need ClassVar
- `SIM102`: Allow nested if for readability
- `SIM117`: Allow nested with for Textual compose patterns

### Constants Module

Use `src/kagan/constants.py` for shared values:

```python
from kagan.constants import COLUMN_ORDER, STATUS_LABELS, PRIORITY_LABELS
from kagan.constants import DEFAULT_DB_PATH, DEFAULT_CONFIG_PATH
```

### Key Architectural Decisions

1. **Single CSS file** - All styles in `kagan.tcss`, no `DEFAULT_CSS` in Python
2. **Built-in Footer** - Use Textual's `Footer` widget, auto-displays from `BINDINGS`
3. **Property assertions** - Use `@property` with `assert` for required state
4. **ModalAction enum** - Use `src/kagan/ui/modals/actions.py` for modal actions
5. **Async database** - All DB operations are async via `aiosqlite`
