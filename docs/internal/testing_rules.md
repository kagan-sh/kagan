# Kagan Testing Rules v2

Hybrid Sociable-Driver Architecture for TUI testing. Synthesizes Fowler's sociable testing, Textual's pilot pattern, and async determinism.

> **Required Reading**: Read before writing or modifying tests for `src/`.

______________________________________________________________________

## 1. Test Classification

| Category        | Marker                    | Purpose                    | Mocking Allowed        |
| --------------- | ------------------------- | -------------------------- | ---------------------- |
| **unit**        | `pytest.mark.unit`        | Pure logic, no I/O         | None (except stdlib)   |
| **integration** | `pytest.mark.integration` | Real filesystem/DB         | External services only |
| **e2e**         | `pytest.mark.e2e`         | Full app via Textual pilot | Network calls only     |
| **snapshot**    | `pytest.mark.snapshot`    | Visual regression          | None                   |

**Rules**: Mock stdlib → UNIT | Mock internal classes → INTEGRATION | Use pilot → E2E | No I/O → UNIT

```python
# WRONG: tests/e2e/test_detect.py with pytest.mark.e2e
def test_windows():
    with patch("...platform.system", return_value="Windows"):  # This is UNIT!
        result = detect_issues()


# CORRECT: tests/unit/test_detect.py with pytest.mark.unit
```

______________________________________________________________________

## 2. Hybrid Sociable-Driver Architecture

| Layer              | %   | Method                   | Target          |
| ------------------ | --- | ------------------------ | --------------- |
| Headless Component | 70% | `app.run_test()` + pilot | State, DOM, CSS |
| PTY Integration    | 20% | Snapshot comparison      | Visual output   |
| E2E Smoke          | 10% | Full binary              | Exit codes      |

```python
# 70% Headless
async def test_ticket_creation(e2e_app):
    async with e2e_app.run_test() as pilot:
        await pilot.press("n")
        await pilot.pause()
        assert pilot.app.screen.query_one("#ticket-editor")


# 20% Snapshot
def test_card_visual(snap_compare):
    assert snap_compare(CardSnapshotApp(tickets), terminal_size=(50, 20))
```

### 2.1 Fowler Sociable Unit Testing

Test widgets with real children. Mock only I/O boundaries.

| Mock                        | Don't Mock                         |
| --------------------------- | ---------------------------------- |
| File I/O, Network, Database | Child widgets, layout, focus chain |

```python
# WRONG: Mocking children
column = KanbanColumn(cards=[MagicMock()])

# CORRECT: Real widget tree
async with e2e_app_with_tickets.run_test() as pilot:
    cards = pilot.app.screen.query(TicketCard)
    assert len(cards) >= 1
```

______________________________________________________________________

## 3. Law of Async Quiescence

**`time.sleep()` is forbidden.** Await event loop idle.

| Framework | Method                                  |
| --------- | --------------------------------------- |
| Textual   | `await pilot.pause()`                   |
| Workers   | `await app.workers.wait_for_complete()` |

```python
# WRONG
await pilot.click("#submit")
time.sleep(0.1)

# CORRECT
await pilot.click("#submit")
await pilot.pause()
```

______________________________________________________________________

## 4. Golden Master Principle

Snapshots are visual contracts. Changes require human review.

```python
FIXED_DATE = datetime(2025, 1, 15, 12, 0, 0)  # Never datetime.now()


def make_ticket(title: str) -> Ticket:
    return Ticket(id="test1234", title=title, created_at=FIXED_DATE, updated_at=FIXED_DATE)


def test_card(snap_compare):
    assert snap_compare(CardSnapshotApp([(make_ticket("Fix bug"), {})]), terminal_size=(50, 20))
```

```bash
UPDATE_SNAPSHOTS=1 uv run pytest tests/snapshot/ --snapshot-update
git diff tests/snapshot/__snapshots__/  # Review before commit
```

______________________________________________________________________

## 5. Accessibility Gate

TAB traversal must visit all interactive elements and return to start.

```python
async def test_focus_chain(e2e_app):
    async with e2e_app.run_test() as pilot:
        await pilot.pause()
        start = pilot.app.focused
        for _ in range(20):
            await pilot.press("tab")
            await pilot.pause()
            if pilot.app.focused == start:
                break
        assert pilot.app.focused == start, "Focus chain not closed"
```

### 5.1 Blind Pilot Mode (Future)

Navigate using semantic labels only, not coordinates or CSS selectors.

______________________________________________________________________

## 6. Boundary Mocking & VCR Pattern

Mock at system boundaries. Use recording over synthetic mocks.

```python
async def test_api(httpx_mock):
    httpx_mock.add_response(url="https://api.example.com/status", json={"status": "ok"})
    result = await check_api_status()
    assert result.status == "ok"
```

______________________________________________________________________

## 7. Clean Room Environment

```python
@pytest.fixture
async def clean_env(tmp_path, monkeypatch):
    monkeypatch.setenv("TERM", "xterm-256color")
    monkeypatch.setenv("HOME", str(tmp_path))
    yield tmp_path


@pytest.fixture
async def state_manager(tmp_path):
    manager = StateManager(tmp_path / "test.db")
    await manager.initialize()
    yield manager
    await manager.close()
```

______________________________________________________________________

## 8. Avoid Tautological Tests

Don't mock A then assert A.

```python
# WRONG
prompt = get_prompt(title="Test")
assert "Test" in prompt

# CORRECT - exception IS the test
get_prompt(title="x", ticket_id="y", desc="z", commits="c", diff="d")
```

______________________________________________________________________

## 9. Test-to-Code Ratio

| Complexity         | Ratio      |
| ------------------ | ---------- |
| Getters/setters    | 0:1        |
| Business logic     | 1:1 to 2:1 |
| Complex algorithms | 2:1 to 3:1 |

**Don't test**: Enum defs, Pydantic defaults, simple properties (type checker handles).

```python
# DON'T TEST - type checker handles
class TicketStatus(Enum):
    BACKLOG = "backlog"  # Don't assert TicketStatus.BACKLOG.value == "backlog"
```

______________________________________________________________________

## 10. Fixture Usage

| Fixture         | Purpose                         |
| --------------- | ------------------------------- |
| `state_manager` | Async StateManager with temp DB |
| `git_repo`      | Initialized git repo            |
| `mock_agent`    | Mock ACP agent                  |
| `e2e_app`       | KaganApp for pilot testing      |

Check `conftest.py` before creating. If used by 2+ files → add to conftest.

```python
# WRONG: Duplicate fixture
@pytest.fixture
def mock_session_manager():  # Already exists in conftest.py!
    return MagicMock()


# CORRECT: Use existing
async def test_merge(mock_session_manager): ...  # From conftest
```

______________________________________________________________________

## 11. Parametrization

```python
# WRONG
async def test_j_moves(): ...
async def test_down_moves(): ...


# CORRECT
@pytest.mark.parametrize("key", ["j", "down"])
async def test_moves_down(key, e2e_app):
    await pilot.press(key)
    await pilot.pause()
```

______________________________________________________________________

## 12. Assertion Guidelines

Test observable behavior, not implementation.

```python
# WRONG
assert editor.read_only

# CORRECT
original = editor.text
await pilot.press("a")
assert editor.text == original  # Can't type = readonly
```

______________________________________________________________________

## 13. File Organization

**Keep test files compact.** Aim for brevity, but never exceed 1000 LOC per file.

| Pattern                      | Use               |
| ---------------------------- | ----------------- |
| `test_{module}.py`           | Main module tests |
| `test_{module}_{feature}.py` | Feature-specific  |

```python
# If a file grows large, consider splitting into classes:
# test_scheduler.py
class TestSchedulerBasics: ...


class TestSchedulerAgent: ...


class TestSchedulerAutoMerge: ...


# Move shared fixtures to conftest.py
```

______________________________________________________________________

## 14. Quick Checklist

- [ ] Correct `pytestmark` (unit/integration/e2e/snapshot)
- [ ] No duplicate fixtures
- [ ] No tautological assertions
- [ ] E2E tests don't mock internals
- [ ] Parametrized where possible
- [ ] File is compact (never exceed 1000 LOC)
- [ ] `await pilot.pause()` after every interaction
- [ ] Snapshots use fixed dates
- [ ] No `time.sleep()` in tests

______________________________________________________________________

## 15. Anti-Pattern Reference

| Anti-Pattern            | Detection                      | Fix                   |
| ----------------------- | ------------------------------ | --------------------- |
| Mock-Heavy Tautology    | Mock A, assert A               | Test behavior         |
| Fixture Duplication     | Same fixture in multiple files | Move to conftest      |
| Misclassified Test      | Unit with `pytest.mark.e2e`    | Correct marker        |
| Over-mocking in E2E     | `patch("module.Class")`        | Mock network only     |
| Sleep Anti-Pattern      | `time.sleep()`                 | `await pilot.pause()` |
| Solitary Widget Testing | Mocking children               | Real widgets          |
| Missing Quiescence      | No `pause()` after action      | Add pause             |
| Snapshot Fatigue        | Ignoring failures              | Review + update       |

______________________________________________________________________

## 16. Directory Structure

```
tests/
  conftest.py
  strategies.py              # Reusable Hypothesis strategies
  helpers/{pages.py, mocks.py, git.py, e2e.py}
  unit/
  integration/
  snapshot/__snapshots__/
  e2e/conftest.py
```

______________________________________________________________________

## 17. Running Tests

```bash
uv run pytest tests/unit/ -v        # Unit only
uv run pytest tests/ -n 0           # Sequential
uv run pytest tests/ -n auto        # Parallel
UPDATE_SNAPSHOTS=1 uv run pytest tests/snapshot/ --snapshot-update
HYPOTHESIS_PROFILE=ci uv run pytest  # CI profile (100 examples)
```

______________________________________________________________________

## 18. pytest-asyncio

Project uses `asyncio_mode = "auto"`:

- No `@pytest.mark.asyncio` needed
- No `event_loop` fixture (deprecated)
- Use `asyncio.get_running_loop()` not `get_event_loop()`

```python
@pytest.fixture
async def state_manager(tmp_path):
    manager = StateManager(tmp_path / "db")
    await manager.initialize()
    yield manager
    await manager.close()


async def test_ui(e2e_app):
    async with e2e_app.run_test() as pilot:
        await pilot.pause()
        await pilot.press("j")
        await pilot.pause()
```

______________________________________________________________________

## 19. Hypothesis Property-Based Testing

Use Hypothesis for inputs with large/infinite domains. Complements example-based tests.

### 19.1 When to Use

| Use Hypothesis                   | Use Example-Based                 |
| -------------------------------- | --------------------------------- |
| Parsers, encoders, serializers   | UI interactions                   |
| State machines, transitions      | Specific edge cases               |
| Functions with many valid inputs | Integration with external systems |
| Invariants that must always hold | Snapshot tests                    |

### 19.2 Profiles

```python
# conftest.py - already configured
settings.register_profile("ci", max_examples=100, deadline=None)
settings.register_profile("dev", max_examples=20, deadline=500)
settings.register_profile("debug", max_examples=10, verbosity=Verbosity.verbose)
```

```bash
HYPOTHESIS_PROFILE=ci uv run pytest   # CI: thorough
HYPOTHESIS_PROFILE=debug uv run pytest  # Debug: verbose output
```

### 19.3 Strategy Design

Define reusable strategies in `tests/strategies.py`. Build composite strategies from atomic ones.

```python
# ATOMIC: Single domain values
valid_ticket_titles = st.text(min_size=1, max_size=200).filter(
    lambda x: x.strip() and "\x00" not in x
)
statuses = st.sampled_from(list(TicketStatus))


# COMPOSITE: Build from atomics
@st.composite
def tickets(draw: st.DrawFn, **overrides) -> Ticket:
    return Ticket(
        title=overrides.get("title", draw(valid_ticket_titles)),
        status=overrides.get("status", draw(statuses)),
    )
```

**Strategy Rules:**

- Filter early: `st.text(...).filter(valid)` not `assume(valid(x))`
- Use `st.sampled_from()` for enums
- Use `@st.composite` for complex objects
- Blacklist problematic characters: `\x00`, `\x1b`, surrogates

### 19.4 Test Structure

```python
from hypothesis import given
from tests.strategies import tickets, valid_ticket_titles


class TestTicketParsing:
    @given(tickets())
    def test_roundtrip_serialization(self, ticket: Ticket):
        """Property: serialize then deserialize = original."""
        serialized = ticket.to_json()
        restored = Ticket.from_json(serialized)
        assert restored == ticket

    @given(valid_ticket_titles)
    def test_title_preserved(self, title: str):
        """Property: title is stored exactly as given."""
        ticket = Ticket.create(title=title, description="")
        assert ticket.title == title
```

### 19.5 Stateful Testing

Use `RuleBasedStateMachine` for state machines with invariants.

```python
from hypothesis.stateful import RuleBasedStateMachine, rule, invariant


class StateMachineTest(RuleBasedStateMachine):
    def __init__(self):
        super().__init__()
        self.state = MyState()

    @invariant()
    def always_valid(self):
        """Invariant checked after every rule."""
        assert self.state.is_consistent()

    @rule()
    def do_action(self):
        """One possible action in the state machine."""
        if self.state.can_act():
            self.state = self.state.act()


TestStateMachine = StateMachineTest.TestCase  # pytest discovers this
```

### 19.6 Common Patterns

```python
# ROUNDTRIP: encode/decode preserves data
@given(plain_text)
def test_ansi_strip_idempotent(self, text: str):
    once = strip_ansi(text)
    twice = strip_ansi(once)
    assert once == twice


# ORACLE: compare against known-good implementation
@given(st.lists(st.integers()))
def test_sort_matches_builtin(self, items: list[int]):
    assert my_sort(items) == sorted(items)


# INVARIANT: property always holds
@given(tickets())
def test_priority_always_valid(self, ticket: Ticket):
    assert ticket.priority in TicketPriority
```

### 19.7 Async Hypothesis Tests

Hypothesis works with async but requires care with fixtures.

```python
@given(tickets())
async def test_db_insert(self, state_manager, ticket: Ticket):
    """Async test with hypothesis - fixture must be function-scoped."""
    await state_manager.create_ticket(ticket)
    retrieved = await state_manager.get_ticket(ticket.id)
    assert retrieved.title == ticket.title
```

**Note:** Database fixtures in hypothesis tests must handle repeated calls. Use `@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])` if needed.

### 19.8 Anti-Patterns

| Anti-Pattern           | Problem                    | Fix                           |
| ---------------------- | -------------------------- | ----------------------------- |
| `assume()` overuse     | Discards too many examples | Filter in strategy definition |
| Slow strategies        | Timeout/deadline failures  | Simplify or increase deadline |
| Flaky assertions       | Non-deterministic failures | Make assertions deterministic |
| Testing implementation | Brittle to refactoring     | Test observable properties    |
| No shrinking           | Hard to debug failures     | Keep `Phase.shrink` enabled   |

### 19.9 Debugging Failures

```python
# Reproduce a specific failure
@given(tickets())
@settings(database=None)  # Disable example database
@example(Ticket(title="failing case", ...))  # Explicit example
def test_with_explicit(self, ticket):
    ...

# Verbose output for debugging
HYPOTHESIS_PROFILE=debug uv run pytest tests/unit/test_x.py -v
```

### 19.10 Quick Reference

```python
# Essential imports
from hypothesis import given, settings, assume, example
from hypothesis import strategies as st
from hypothesis.stateful import RuleBasedStateMachine, rule, invariant

# Common strategies
st.text(min_size=1, max_size=100)           # Strings
st.integers(min_value=0, max_value=1000)     # Bounded ints
st.sampled_from(list(MyEnum))                # Enum values
st.lists(st.integers(), min_size=1)          # Non-empty lists
st.one_of(st.none(), st.text())              # Optional values
st.builds(MyClass, field=st.text())          # Objects from constructors

# Decorators
@given(strategy)                              # Generate inputs
@settings(max_examples=50, deadline=1000)     # Override settings
@example(specific_value)                      # Always test this case
```

______________________________________________________________________

## 20. Textual Widget Message Testing

When testing widget message posting, use `patch.object` context manager to avoid test hangs.

```python
# WRONG - causes Textual message loop hang
widget.post_message = lambda m: messages.append(m)  # Never do this!

# CORRECT - restores original method after test
from unittest.mock import patch
from tests.helpers.mocks import MessageCapture

capture = MessageCapture()
with patch.object(widget, "post_message", capture):
    widget.action_select()

msg = capture.assert_single(MyWidget.Completed)  # Exactly one message
msg = capture.assert_contains(MyWidget.Approved)  # At least one of type
```

**Why:** Permanently replacing `post_message` breaks Textual's internal message queue, causing `async with app.run_test()` to hang on exit.
