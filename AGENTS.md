# AGENTS.md — Coding Agent Instructions for Kagan

Kagan is an AI-powered Kanban TUI for autonomous development workflows.
Built with Python 3.12+, Textual, SQLModel, and the Agent Control Protocol (ACP).

## Build, Lint, and Test Commands

```bash
uv sync                         # Install dependencies
uv run poe dev                  # Run app with hot reload
uv run poe fix                  # Auto-fix lint + format (ALWAYS run before committing)
uv run poe lint                 # Ruff linter (check only)
uv run poe format               # Ruff formatter only
uv run poe typecheck            # Pyrefly type checker
uv run poe check                # Full suite: lint + typecheck + test
```

### Running Tests

```bash
uv run pytest tests/ -v                                                   # All tests
uv run pytest tests/features/test_agent_automation.py -v                  # Single file
uv run pytest tests/features/test_agent_automation.py::TestClass::test_x -v  # Single test
uv run pytest -k "test_name_pattern" -v                                   # Pattern match
uv run pytest -m unit -v              # By marker: unit, integration, e2e, snapshot, property, slow
uv run poe test-snapshot              # Snapshot tests (MUST run sequentially, no parallel)
uv run poe test-snapshot-update       # Update snapshots
```

## Project Structure

```
src/kagan/
├── app.py, bootstrap.py    # KaganApp + AppContext (DI container)
├── constants.py            # Shared constants (COLUMN_ORDER, STATUS_LABELS)
├── keybindings.py          # ALL keybindings (single file)
├── config.py               # KaganConfig (Pydantic-based)
├── adapters/db/            # schema.py (SQLModel), repositories.py
├── core/models/            # entities.py, enums.py, policies.py
├── services/               # Protocol interfaces + *Impl classes
├── agents/                 # Prompt building, signal parsing, agent coordination
├── acp/                    # Agent Control Protocol (JSON-RPC over subprocess)
├── ui/screens/             # KaganScreen subclasses (kanban/, planner/, task_editor)
├── ui/widgets/             # Reusable widgets (card, column, chat_panel)
├── ui/modals/              # ModalScreen subclasses (review, diff, settings)
└── styles/kagan.tcss       # ALL CSS — single source of truth
tests/
├── conftest.py             # Core fixtures (state_manager, event_bus, task_factory)
├── helpers/                # wait.py, mocks.py, journey_runner.py
├── features/               # Feature/E2E tests (user journeys)
├── snapshots/              # Visual regression tests
└── property/               # Hypothesis property-based tests
```

## Code Style

### Ruff Configuration

Line length: **100**. Target: **py312**. Rules: `E, F, I, UP, B, SIM, TCH, RUF`.
Intentionally ignored: `RUF012` (Textual class attrs), `RUF006` (fire-and-forget tasks),
`SIM102`/`SIM117` (nested if/with for readability).

### Imports

```python
from __future__ import annotations  # ALWAYS first line in every file

from datetime import datetime  # 1. Standard library
from typing import TYPE_CHECKING, cast

from pydantic import BaseModel  # 2. Third-party
from textual.app import ComposeResult

from kagan.constants import COLUMN_ORDER  # 3. Local

if TYPE_CHECKING:  # 4. Type-only imports
    from kagan.app import KaganApp
```

### Types

- Always annotate function signatures and class attributes
- Use `X | None` (not `Optional[X]`); use `TYPE_CHECKING` for circular imports
- Use `cast("TargetType", value)` for type narrowing

### Naming

| Element   | Convention   | Example                           |
| --------- | ------------ | --------------------------------- |
| Classes   | PascalCase   | `TaskCard`, `KanbanScreen`        |
| Functions | snake_case   | `get_all_tasks`, `_refresh_board` |
| Private   | `_` prefix   | `_repo`, `_build_prompt`          |
| Constants | UPPER_SNAKE  | `COLUMN_ORDER`, `DEFAULT_DB_PATH` |
| Enums     | UPPER values | `TaskStatus.BACKLOG`              |

### Error Handling

- Prefer result objects (`success: bool` + `error`) for expected failures over exceptions
- Use specific exception types, never bare `except:`; use `contextlib.suppress()` when intentional
- Keep modules **150–250 LOC**; test files **< 200 LOC**

## Textual UI Patterns

### CSS: Centralized Only

**All styles go in `src/kagan/styles/kagan.tcss`**. Never use `DEFAULT_CSS` or inline CSS.

### Messages, Handlers, Reactives

```python
@dataclass
class Selected(Message):
    task: Task


@on(Button.Pressed, "#save-btn")
def on_save(self) -> None:
    self.action_submit()


tasks: reactive[list[Task]] = reactive(list, recompose=True)
```

### Screens and Modals

- Screens inherit from `KaganScreen` (in `ui/screens/base.py`)
- Modals use `ModalScreen[ReturnType]` for typed return values
- All keybindings defined centrally in `keybindings.py`

## Service Layer

- Define interfaces with `Protocol`, implement with `*Impl` suffix
- Constructor-based DI via `AppContext` (`bootstrap.py`). All I/O is async
- All DB access through `TaskRepository`; cross-service comms via `EventBus` events

```python
class TaskService(Protocol):
    async def create_task(self, title: str) -> Task: ...


class TaskServiceImpl:
    def __init__(self, repo: TaskRepository, event_bus: EventBus) -> None:
        self._repo = repo
        self._events = event_bus
```

## Testing

- Tests organized by **user-facing features**, not implementation layers
- Snapshot tests use `snap_compare()` fixture; run sequentially only
- Use `wait_for_screen()` / `wait_for_widget()` from `tests/helpers/wait.py`
  (never `wait_for_workers()` — orphaned workers cause timeouts)
- E2E tests use real DB + mocked agents (`MockAgent`, `MockAgentFactory`)
- Key fixtures: `state_manager`, `event_bus`, `task_service`, `task_factory`, `git_repo`

## Git Commit Rules

- Disable GPG signing: `git config commit.gpgsign false`
- Conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`, `perf:`
- `feat:` triggers minor version bump; `fix:`/`perf:`/`docs:` trigger patch
- Keep commits atomic and focused
- Always run `uv run poe fix` before committing
