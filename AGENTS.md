# AGENTS.md - Coding Agent Instructions for Kagan

This document provides essential guidelines for AI coding agents working in this repository.

## Project Overview

Kagan is an AI-powered Kanban TUI for autonomous development workflows. Built with Python 3.12+, Textual framework, and SQLModel for persistence.

## Build, Lint, and Test Commands

```bash
# Install dependencies
uv sync

# Run the application
uv run kagan                    # Launch TUI
uv run poe dev                  # Development mode with hot reload

# Linting & Formatting (use these before committing)
uv run poe lint                 # Ruff linter (check only)
uv run poe format               # Ruff formatter
uv run poe fix                  # Auto-fix lint issues + format (RECOMMENDED)
uv run poe typecheck            # Pyrefly type checker
uv run poe check                # Full suite: lint + typecheck + test

# Testing
uv run pytest tests/ -v                                              # All tests
uv run pytest tests/features/test_agent_automation.py -v             # Single file
uv run pytest tests/features/test_agent_automation.py::TestClass -v  # Single class
uv run pytest tests/features/test_agent_automation.py::TestClass::test_method -v  # Single test
uv run pytest -k "test_name_pattern" -v                              # Pattern match

# Snapshot tests (MUST run sequentially - no parallel)
uv run poe test-snapshot                         # Run snapshot tests
uv run poe test-snapshot-update                  # Update snapshots
UPDATE_SNAPSHOTS=1 uv run pytest tests/snapshots/ -n 0 --snapshot-update
```

## Code Style Guidelines

### Imports

Always use this order with blank lines between groups:

```python
from __future__ import annotations  # ALWAYS first line

# Standard library
from datetime import datetime
from typing import TYPE_CHECKING, cast

# Third-party
from pydantic import BaseModel
from textual.app import ComposeResult

# Local imports
from kagan.constants import COLUMN_ORDER
from kagan.core.models.entities import Task

if TYPE_CHECKING:
    from kagan.app import KaganApp  # Type-only imports go here
```

### Type Annotations

- Always annotate function signatures and class attributes
- Use `X | None` union syntax, NOT `Optional[X]`
- Use `TYPE_CHECKING` block for imports only needed for type hints
- Use `cast()` for type narrowing: `return cast("KaganApp", self.app)`

### Naming Conventions

| Type      | Convention        | Example                    |
| --------- | ----------------- | -------------------------- |
| Classes   | PascalCase        | `TaskCard`, `KanbanScreen` |
| Functions | snake_case        | `get_all_tasks`            |
| Private   | underscore prefix | `_refresh_board`           |
| Constants | UPPER_SNAKE       | `COLUMN_ORDER`             |
| Enums     | PascalCase/UPPER  | `TaskStatus.BACKLOG`       |

### Error Handling

- Use specific exception types, not bare `except:`
- Log errors with context before re-raising
- For async operations, use `try/except` with proper cleanup in `finally`

### Textual-Specific Patterns

```python
# Messages as dataclasses
@dataclass
class Selected(Message):
    task: Task


# Event handlers with @on decorator
@on(Button.Pressed, "#save-btn")
def on_save(self) -> None:
    self.action_submit()


# Reactive attributes
tasks: reactive[list[Task]] = reactive(list, recompose=True)


# Widget IDs in __init__
def __init__(self, task: Task, **kwargs) -> None:
    super().__init__(id=f"card-{task.id}", **kwargs)
```

### CSS Organization (Centralized)

Kagan follows a **centralized CSS strategy** (JiraTUI-style):

- **All new styles** go in `src/kagan/styles/kagan.tcss`
- Avoid `DEFAULT_CSS`, `CSS`, and screen-level `CSS_PATH` for new work
- `KaganApp.CSS_PATH` points to the single global stylesheet
- If a screen/widget needs styles, add a clearly labeled section in `kagan.tcss`
- Use CSS variables for theming: `$primary`, `$error`, `$text-muted`
- Use semantic color aliases: `$priority-high: $error`

```tcss
/* === Welcome Screen === */
WelcomeScreen {
    align: center middle;
}

#welcome-container {
    width: 80;
    border: solid $primary;
    padding: 1 2;
}
```

Existing embedded CSS in legacy widgets may remain, but **do not add new inline CSS** without explicit approval.

### Service Pattern

```python
class TaskService(Protocol):
    """Service interface."""

    async def create_task(self, title: str) -> Task: ...


class TaskServiceImpl:
    """Concrete implementation."""

    def __init__(self, repo: TaskRepository, event_bus: EventBus) -> None:
        self._repo = repo
        self._events = event_bus
```

## Project Structure

```
src/kagan/
├── app.py              # Main KaganApp class
├── bootstrap.py        # Dependency injection / AppContext
├── constants.py        # Shared constants (use this for magic values)
├── adapters/           # External interfaces (DB, git, executors)
├── core/models/        # Domain entities and enums
├── services/           # Business logic layer
├── ui/                 # Screens, widgets, modals
└── styles/kagan.tcss   # ALL CSS (single source of truth)

tests/
├── conftest.py         # Shared fixtures
├── helpers/            # Test utilities
├── features/           # Feature tests
└── snapshots/          # Visual regression tests
```

## Testing Guidelines

### Test Organization

Tests are organized by user-facing features, not implementation layers:

```python
class TestSignalParsing:
    """Agent signals are correctly parsed from output."""

    def test_parse_complete_signal(self):
        """<complete/> signal is recognized."""
        output = "Task finished. <complete/>"
        result = parse_signal(output)
        assert result.signal == Signal.COMPLETE
```

### Key Fixtures (from `tests/conftest.py`)

- `state_manager` - Temporary TaskRepository
- `event_bus` - InMemoryEventBus for testing
- `task_service` - TaskServiceImpl instance
- `task_factory` - Factory for creating Task objects
- `git_repo` - Initialized git repository
- `mock_agent`, `mock_agent_factory` - Mock ACP agents

### Test Markers

```bash
pytest -m "not slow"        # Skip slow tests
pytest -m unit              # Pure logic tests only
pytest -m integration       # Tests with real filesystem/DB
pytest -m snapshot          # Visual regression tests
pytest -m e2e               # Full application tests
```

## Git Commit Rules

- **CRITICAL**: Disable GPG signing in agent workflows:
  ```bash
  git config commit.gpgsign false
  ```
- Use conventional commit format: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`
- Keep commits atomic and focused

## Ruff Configuration

Line length: 100 characters. Key rules enabled:

- `E`, `F` - pycodestyle errors, pyflakes
- `I` - isort import sorting
- `UP` - pyupgrade
- `B` - bugbear
- `SIM` - simplify
- `TCH` - type-checking imports
- `RUF` - Ruff-specific rules

Ignored rules (intentional):

- `RUF012` - Textual class attributes don't need ClassVar
- `RUF006` - Allow fire-and-forget asyncio.create_task
- `SIM102`, `SIM117` - Allow nested if/with for readability

## Key Rules Summary

1. **CSS: Centralized** - All new styles in `src/kagan/styles/kagan.tcss` (no inline CSS)
1. **Async database** - All DB operations via TaskRepository
1. **Constants module** - Use `kagan.constants` for shared values
1. **Module size** - Keep modules ~150-250 LOC; test files < 200 LOC
1. **Protocol-based services** - Define interfaces with Protocol, implement with `*Impl`
1. **Event-driven** - Use domain events for cross-service communication
1. **Run `uv run poe fix`** - Before committing, always run the fix command
