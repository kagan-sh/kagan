# Kagan Testing Rules

Guidelines for writing effective, maintainable tests. Follow these rules to avoid common anti-patterns that waste effort and create maintenance burden.

> **Required Reading**: AI agents and developers MUST read this before writing or modifying tests for code in `src/`.

______________________________________________________________________

## 1. Test Classification

### Categories

| Category        | Marker                    | Purpose                    | Mocking Allowed        |
| --------------- | ------------------------- | -------------------------- | ---------------------- |
| **unit**        | `pytest.mark.unit`        | Pure logic, no I/O         | None (except stdlib)   |
| **integration** | `pytest.mark.integration` | Real filesystem/DB         | External services only |
| **e2e**         | `pytest.mark.e2e`         | Full app via Textual pilot | Network calls only     |
| **snapshot**    | `pytest.mark.snapshot`    | Visual regression          | None                   |

### Classification Rules

1. **If you mock `platform.system`, `shutil.which`, or stdlib** -> UNIT test
1. **If you mock internal classes (`patch("module.ClassName")`)** -> INTEGRATION, not E2E
1. **If you use `pilot.press()`, `pilot.click()`** -> E2E
1. **If no I/O at all** -> UNIT

### WRONG: Unit test in E2E folder

```python
# tests/e2e/test_detect.py - WRONG LOCATION
pytestmark = pytest.mark.e2e  # WRONG MARKER


def test_windows_detection():
    with patch("...platform.system", return_value="Windows"):  # This is a UNIT test!
        result = detect_issues()
```

### CORRECT: Proper classification

```python
# tests/unit/test_detect.py
pytestmark = pytest.mark.unit


def test_windows_detection():
    with patch("...platform.system", return_value="Windows"):
        result = detect_issues()
```

______________________________________________________________________

## 2. Avoid Tautological Tests

### Definition

A tautological test mocks input A, then asserts output equals A. These tests provide zero value - they test the mock, not the code.

### WRONG: Tautological test

```python
def test_get_review_prompt_formats_correctly(self):
    prompt = get_review_prompt(
        title="Test Ticket",  # You set this
    )
    assert "Test Ticket" in prompt  # Trivially true: str.format() works
```

### CORRECT: Test behavior, not string containment

```python
def test_get_review_prompt_has_valid_placeholders():
    # Will raise KeyError if placeholders are missing from template
    get_review_prompt(title="x", ticket_id="y", description="z", commits="c", diff_summary="d")
    # No assertion needed - exception IS the test
```

### WRONG: Testing that constants equal themselves

```python
@pytest.mark.parametrize(
    ("error_cls", "code"),
    [(ParseError, -32700), (InvalidRequest, -32600)],
)
def test_error_codes(self, error_cls, code):
    assert error_cls().code == code  # Just restating source code
```

### CORRECT: Test error behavior

```python
def test_parse_error_is_json_serializable():
    err = ParseError("bad json")
    assert json.loads(err.to_json())["code"] == -32700
```

______________________________________________________________________

## 3. Test-to-Code Ratio

### Guidelines

| Code Complexity        | Acceptable Test:Code Ratio |
| ---------------------- | -------------------------- |
| Simple getters/setters | 0:1 (don't test)           |
| Trivial conditionals   | 0.2:1                      |
| Business logic         | 1:1 to 2:1                 |
| Complex algorithms     | 2:1 to 3:1                 |
| Parsing/serialization  | 1:1 to 1.5:1               |

### Red Flags

- 10+ tests for a 20-line function -> Over-testing
- Test file larger than source file for trivial code -> Reconsider

### What NOT to Test (type checker catches these)

- Enum values equal their definitions
- Pydantic model defaults
- Simple property returns
- Class inheritance

______________________________________________________________________

## 4. Fixture Usage

### Rule: Use conftest.py fixtures, don't duplicate

### WRONG: Duplicate fixture in test file

```python
# tests/integration/test_scheduler_automerge.py
@pytest.fixture
def mock_session_manager():  # DUPLICATE - already in conftest.py!
    manager = MagicMock()
    manager.kill_session = AsyncMock()
    return manager
```

### CORRECT: Use conftest fixture

```python
# tests/integration/test_scheduler_automerge.py
async def test_auto_merge(self, mock_session_manager):  # From conftest.py
    ...
```

### Available Fixtures (USE THESE)

| Fixture                 | Source      | Purpose                          |
| ----------------------- | ----------- | -------------------------------- |
| `state_manager`         | conftest.py | Async StateManager with temp DB  |
| `git_repo`              | conftest.py | Initialized git repo with commit |
| `mock_agent`            | conftest.py | Mock ACP agent                   |
| `mock_worktree_manager` | conftest.py | Mock WorktreeManager             |
| `mock_session_manager`  | conftest.py | Mock SessionManager              |
| `config`                | conftest.py | Test KaganConfig                 |
| `e2e_project`           | conftest.py | Full project setup for E2E       |
| `e2e_app`               | conftest.py | KaganApp ready for pilot testing |

### Creating New Fixtures

1. Check if fixture exists in `tests/conftest.py` first
1. Check `tests/helpers/mocks.py` for factory functions
1. If creating new fixture used by 2+ files -> add to conftest.py
1. If creating mock factory -> add to `tests/helpers/mocks.py`

______________________________________________________________________

## 5. E2E Test Rules

### Golden Rule

**E2E tests mock at boundaries (network), not internals.**

### WRONG: Mock internal class in E2E

```python
# This is NOT an E2E test - it's integration at best
with patch("kagan.agents.scheduler.Agent", return_value=mock_agent):
    await pilot.press("enter")
```

### CORRECT: Test real behavior, mock network

```python
# Use httpx_mock for external API calls only
httpx_mock.add_response(url="https://api.example.com", json={"version": "2.0"})
await pilot.press("enter")
```

### Selector Guidelines

| Avoid                            | Prefer                              |
| -------------------------------- | ----------------------------------- |
| `.issue-card` (CSS class)        | `#issue-card-windows` (semantic ID) |
| `list(app.query(...))` + index   | Helper function in pages.py         |
| `assert len(cards) == 3` (exact) | `assert len(cards) >= 1` (flexible) |

### Use Page Helpers

```python
from tests.helpers.pages import navigate_to_kanban, create_ticket_via_ui


async def test_create_ticket(pilot):
    await navigate_to_kanban(pilot)
    await create_ticket_via_ui(pilot, "My ticket")
```

______________________________________________________________________

## 6. Parametrization

### Rule: Use `@pytest.mark.parametrize` to reduce duplication

### WRONG: Duplicate tests for variations

```python
async def test_j_moves_down(self):
    await pilot.press("j")
    ...


async def test_down_arrow_moves_down(self):
    await pilot.press("down")
    ...
```

### CORRECT: Parametrize

```python
@pytest.mark.parametrize("key", ["j", "down"])
async def test_moves_focus_down(self, key, e2e_app):
    await pilot.press(key)
    ...
```

### When to Parametrize

- Same assertion, different inputs -> Parametrize
- Same flow, different triggers -> Parametrize
- Fundamentally different behaviors -> Separate tests

______________________________________________________________________

## 7. Assertion Guidelines

### Test Observable Behavior, Not Implementation

### WRONG: Assert on internal property

```python
editor = modal.query_one("#editor", TextArea)
assert editor.read_only  # Implementation detail
```

### CORRECT: Assert on user-visible behavior

```python
editor = modal.query_one("#editor", TextArea)
original_text = editor.text
await pilot.press("a")
assert editor.text == original_text  # Can't type = effectively readonly
```

### WRONG: Assert mock was called (only)

```python
mock_worktree_manager.merge_to_main.assert_called_once()
# What if merge_to_main was called but failed? Test still passes!
```

### CORRECT: Assert on result state

```python
ticket = await state_manager.get_ticket(ticket_id)
assert ticket.status == TicketStatus.DONE
# AND optionally verify mock was called
mock_worktree_manager.merge_to_main.assert_called_once()
```

______________________________________________________________________

## 8. File Organization

### Maximum LOC per Test File: 250

### Naming Conventions

| Pattern                      | Use For                      |
| ---------------------------- | ---------------------------- |
| `test_{module}.py`           | Main tests for a module      |
| `test_{module}_{feature}.py` | Tests for a specific feature |

### WRONG: Split files with duplicated fixtures

```
test_scheduler_basics.py      (has scheduler fixture)
test_scheduler_agent.py       (duplicates scheduler fixture)
test_scheduler_automerge.py   (duplicates scheduler fixture)
```

### CORRECT: Single file with shared fixtures OR use conftest

```
test_scheduler.py
  class TestSchedulerBasics
  class TestSchedulerAgent
  class TestSchedulerAutoMerge
```

Or if file exceeds 250 LOC, split but move shared fixtures to conftest.py.

______________________________________________________________________

## 9. Quick Checklist

Before submitting tests, verify:

- [ ] Correct `pytestmark` for test category (unit/integration/e2e/snapshot)
- [ ] No duplicate fixtures (check conftest.py first)
- [ ] No tautological assertions (mock input != assert same output)
- [ ] No testing trivial logic (type checker covers it)
- [ ] E2E tests don't mock internal classes
- [ ] Parametrized where possible
- [ ] File under 250 LOC
- [ ] Uses helpers from `tests/helpers/` where available
- [ ] Assertions verify outcome state, not just mock calls

______________________________________________________________________

## 10. Anti-Pattern Reference

| Anti-Pattern                    | Detection                          | Fix                                  |
| ------------------------------- | ---------------------------------- | ------------------------------------ |
| **Mock-Heavy Tautology**        | Mock A, assert A                   | Test behavior, not echoed values     |
| **Fixture Duplication**         | Same fixture in multiple files     | Move to conftest.py                  |
| **Misclassified Test**          | Unit test with `pytest.mark.e2e`   | Use correct marker and location      |
| **Trivial Logic Testing**       | Testing `if x == Y` returns Y      | Delete - type checker handles it     |
| **Over-mocking in E2E**         | `patch("module.Class")` in E2E     | Mock at network boundary only        |
| **Assertion on Implementation** | `assert obj._internal_prop`        | Assert user-visible outcome          |
| **Duplicate Scenarios**         | Same test logic, different trigger | Parametrize                          |
| **Exact Count Assertions**      | `assert len(items) == 3`           | `assert len(items) >= 1` or semantic |

______________________________________________________________________

## 11. Test Directory Structure

```
tests/
  conftest.py              # Shared fixtures (state_manager, git_repo, mocks)
  helpers/
    __init__.py
    pages.py               # E2E page helpers (navigate_to_kanban, etc.)
    mocks.py               # Mock factory functions
    git.py                 # Git test utilities
    e2e.py                 # E2E utilities
  unit/                    # Pure logic tests, no I/O
    test_models.py
    test_ansi.py
    test_jsonrpc.py
    ...
  integration/             # Real filesystem/DB, mocked externals
    test_database.py
    test_worktree.py
    test_scheduler.py
    ...
  snapshot/                # Visual regression
    test_snapshots.py
    __snapshots__/
  e2e/                     # Full app tests via Textual pilot
    conftest.py            # E2E-specific fixtures
    test_navigation.py
    test_ticket_crud.py
    ...
```

______________________________________________________________________

## 12. Running Tests

```bash
# Run by category
uv run pytest tests/unit/ -v           # Unit tests only
uv run pytest tests/integration/ -v    # Integration only
uv run pytest tests/e2e/ -v            # E2E only
uv run pytest -m unit                  # Using markers

# Full suite (sequential - most reliable)
uv run pytest tests/ -n 0

# Full suite (parallel - faster but may have flaky tests)
uv run pytest tests/ -n auto

# Update snapshots
UPDATE_SNAPSHOTS=1 uv run pytest tests/snapshot/ --snapshot-update
```

______________________________________________________________________

## 13. pytest-asyncio Best Practices

### Configuration

The project uses pytest-asyncio v1.3.0+ with `asyncio_mode = "auto"`:

```toml
[tool.pytest.ini_options]
asyncio_mode = "auto"
```

This means:

- **No `@pytest.mark.asyncio` needed** - all `async def test_*` functions automatically run as async tests
- **No `@pytest_asyncio.fixture` needed** - all async fixtures are auto-detected
- **No `event_loop` fixture** - deprecated and removed in v1.0.0

### Patterns to AVOID

| Pattern                                 | Issue                         | Use Instead                                |
| --------------------------------------- | ----------------------------- | ------------------------------------------ |
| `asyncio.get_event_loop()`              | Deprecated in Python 3.10+    | `asyncio.get_running_loop()` in async code |
| `asyncio.new_event_loop()` for futures  | Creates orphan loop           | Create futures in async context            |
| `@pytest.mark.asyncio`                  | Unnecessary with auto mode    | Just use `async def test_*`                |
| Custom `event_loop` fixture             | Removed in pytest-asyncio 1.0 | Use default loop management                |
| `loop.run_until_complete()` in fixtures | Deprecated pattern            | Use async fixtures with `await`            |

### Creating Futures for Testing

```python
# WRONG - creates future on orphan loop
def _create_future():
    loop = asyncio.new_event_loop()
    return loop.create_future()


# CORRECT - create futures in async context
@pytest.fixture
async def result_future():
    return asyncio.get_running_loop().create_future()


async def test_something(result_future):
    # Use the fixture
    result_future.set_result("done")
    assert result_future.done()
```

### Async Fixture Cleanup

```python
# WRONG - deprecated pattern in sync fixture
@pytest.fixture
def state_manager(tmp_path):
    manager = StateManager(tmp_path / "db")
    yield manager
    asyncio.get_event_loop().run_until_complete(manager.close())  # Deprecated!


# CORRECT - async fixture with proper cleanup
@pytest.fixture
async def state_manager(tmp_path):
    manager = StateManager(tmp_path / "db")
    await manager.initialize()
    yield manager
    await manager.close()  # Clean async cleanup
```

### Textual Testing Compatibility

Textual's `run_test()` creates its own event loop context. This works seamlessly with pytest-asyncio because:

1. Each test gets a fresh event loop from pytest-asyncio
1. `run_test()` is an async context manager that manages Textual's internal loop

```python
async def test_ui_interaction(self, e2e_app: KaganApp):
    async with e2e_app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()  # IMPORTANT: Wait for message processing
        await pilot.press("j")
        await pilot.pause()
        # Assert on app state
```

**Key rule**: Always use `await pilot.pause()` after interactions to allow Textual's message queue to process.
