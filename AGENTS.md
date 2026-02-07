# Agent Development Guide for Kagan

Comprehensive instructions for AI coding agents working in the Kagan repository.

## Project Overview

Kagan is an AI-powered Kanban TUI for autonomous development workflows built with Python 3.12+, Textual, and async/await patterns.

## Commands

### Setup & Running

```bash
uv sync --dev                    # Install dependencies
uv run pre-commit install        # Install pre-commit hooks
uv run poe dev                   # Run with live reload
uv run kagan                     # Run normally
uv run kagan mcp                 # Run as MCP server
```

### Testing

```bash
# Run all tests (parallel by default)
uv run pytest tests/

# Run single test file
uv run pytest tests/e2e/test_critical_flows.py

# Run single test function
uv run pytest tests/e2e/test_critical_flows.py::TestAutoTicketLifecycle::test_auto_full_lifecycle

# Run by marker
uv run pytest -m unit            # Pure logic
uv run pytest -m integration     # Real filesystem/DB
uv run pytest -m e2e            # Full app
uv run pytest -m "not slow"     # Exclude slow tests

# Sequential (debugging)
uv run pytest tests/ -n 0
```

### Linting & Type Checking

```bash
uv run poe lint                  # Lint with ruff
uv run poe format                # Format with ruff
uv run poe typecheck             # Type check with pyrefly
uv run poe fix                   # Fix lint + format
uv run poe check                 # Lint + typecheck + test
uv run pre-commit run --all-files
```

## Code Style

### Import Order (Ruff/isort-compatible)

```python
from __future__ import annotations  # Always first

import asyncio  # Standard library
from pathlib import Path
from typing import TYPE_CHECKING, Any

from pydantic import BaseModel  # Third-party
from textual.app import App

from kagan.config import KaganConfig  # Local

if TYPE_CHECKING:  # At end
    from collections.abc import Callable
```

### Type Annotations (MANDATORY)

- Use Python 3.12+ syntax: `list[str]`, `dict[str, Any]`, `str | None`
- Use `type` aliases: `type OS = Literal["linux", "macos", "windows", "*"]`
- Use `TYPE_CHECKING` blocks to avoid circular imports
- All functions must have type annotations

### Formatting (Ruff enforced)

- **Line length**: 100 chars
- **Quotes**: Double quotes
- **Indentation**: 4 spaces
- **Docstrings**: Google-style with triple double-quotes

### Naming

- Functions/variables: `snake_case`
- Classes: `PascalCase`
- Constants: `UPPER_SNAKE_CASE`
- Private: `_private_method`

### Error Handling

- Define custom exceptions: `class WorktreeError(Exception): ...`
- Use dataclasses for error details (see `GitError` in `git_utils.py`)
- Always include context in exception messages
- Use `try/finally` with `await manager.close()`

### Async/Await (CRITICAL)

- **All I/O must be async** (database, subprocess, file I/O)
- Use `asyncio.create_subprocess_exec`, never `subprocess.run`
- Use `aiosqlite`, never blocking `sqlite3`
- Use `asyncio.Lock` for concurrent access
- Use `async with` for resource management

### Pydantic (v2 API)

- Use `BaseModel`, `Field`, `ConfigDict`
- Use `@field_validator` decorator
- Use `model_validate`, `model_dump` methods

### Textual UI

- Use `BINDINGS` class attribute (no ClassVar needed)
- Use `compose()` for widget hierarchy
- CSS in `src/kagan/styles/*.tcss`
- Use `reactive()` for state
- Use `Signal[T]` for pub/sub

### Database

- Use `StateManager` for all DB access
- Use predefined queries from `queries.py` (no raw SQL)
- Always close: `await manager.close()` or `async with`
- Register callbacks: `manager.set_status_change_callback()`

### Git

- Use async functions from `git_utils.py`
- Check for `GitError` in results
- Never use blocking git commands
- Use worktrees for parallel execution

## Testing

### Markers

- `@pytest.mark.unit` - Pure logic
- `@pytest.mark.integration` - Real filesystem/DB
- `@pytest.mark.e2e` - Full app
- `@pytest.mark.snapshot` - Visual regression
- `@pytest.mark.property` - Hypothesis property-based
- `@pytest.mark.slow` - Tests >5 seconds

### Patterns

- Use descriptive class/function names: `TestAutoTicketLifecycle::test_auto_full_lifecycle`
- Use fixtures from `conftest.py` and helpers from `tests/helpers/`
- Mark async tests: `async def test_...`
- Use `await pilot.pause()` in Textual tests
- Configure Hypothesis: `settings.register_profile("aggressive", max_examples=100, deadline=None)`
- Reduce `max_examples=10` for DB operations

## Git Commits (Conventional Commits)

**Format**: `<type>(<scope>): <description>`

**Types** (semantic-release):

- `feat` - New feature (minor bump)
- `fix` - Bug fix (patch bump)
- `perf` - Performance (patch bump)
- `docs` - Documentation (patch bump)
- `refactor`, `test`, `chore`, `ci`, `build`, `style` - No version bump

**Examples**:

```
feat: add auto-merge capability for REVIEW tickets
fix: resolve UI freezes from blocking git operations
perf(ci): optimize test parallelization
refactor(tests): reduce httpx_mock usage
ci: fix cd failing due to lack of git profile
```

## Project Structure

```
src/kagan/
├── database/           # SQLite (aiosqlite): manager.py, models.py, queries.py, schema.sql
├── agents/             # Agent management: scheduler, worktree, installer
├── ui/                 # Textual components: screens/, modals/, forms/, widgets/
├── acp/                # Autonomous Coding Protocol (MCP server)
├── cli/                # Click CLI commands
├── lifecycle/          # Ticket lifecycle logic
├── sessions/           # tmux session management
├── styles/             # Textual CSS
├── app.py              # Main Textual App
├── config.py           # TOML configuration
├── git_utils.py        # Async git operations
└── __main__.py         # CLI entry point

tests/
├── e2e/                # End-to-end tests
├── integration/        # Real filesystem/DB tests
├── property/           # Hypothesis tests
├── snapshot/           # Visual regression
├── helpers/            # Test helpers & page objects
└── conftest.py         # Pytest fixtures
```

## Notes

- Python 3.12+ required
- Use `uv` for all package management
- Pre-commit hooks must pass
- CI: Ubuntu + macOS (full matrix on main)
- System deps: `tmux`, `git`
