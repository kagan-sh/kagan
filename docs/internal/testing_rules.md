# Testing Rules & Guidelines

## Overview

Kagan uses pytest with parallel execution, Textual snapshot testing, and property-based testing (Hypothesis) for comprehensive coverage.

**Phase 4 Status**:

- **Tests**: 119 (111 → 119, +8)
- **Coverage**: 44% (42% → 44%, +2%)
- **Execution**: 6.56s (within 30s budget)
- **Pass rate**: 100% (119/119 passing, 0 skipped)

**Target**: ~120 tests, 50% coverage by Phase 5.

## Test Distribution

| Type        | Count   | Description                   |
| ----------- | ------- | ----------------------------- |
| Unit        | 46      | Pure logic, no I/O            |
| Integration | 10      | Real filesystem/DB            |
| E2E         | 42      | Full app flows (39→42, +3)    |
| Snapshot    | 13      | Visual regression (8→13, +5)  |
| Property    | 8       | Hypothesis-based              |
| **Total**   | **119** | **87→119, +32 since Phase 3** |

## Markers

```python
@pytest.mark.unit            # Pure logic
@pytest.mark.integration     # Real filesystem/DB
@pytest.mark.e2e            # Full app
@pytest.mark.snapshot       # Visual regression
@pytest.mark.property       # Hypothesis
@pytest.mark.slow           # Tests >5 seconds
```

## Testing Philosophy

1. **Parallel by default**: Tests run with `pytest-xdist` (-n auto)
1. **Async-first**: Use `async def test_...` for I/O
1. **Descriptive names**: `TestAutoTicketLifecycle::test_auto_full_lifecycle`
1. **Fixtures**: Use `conftest.py` and `tests/helpers/`
1. **No skipped tests**: All tests must pass (Phase 4: 0 skipped)

## Phase 4 Achievements

### 1. Agent Factory Pattern

- Dependency injection for testability
- Protocol: `AgentFactory` in `src/kagan/agents/agent_factory.py`
- Default factory returns real Agent
- Tests inject mock factories
- Un-skipped 3 scheduler edge case tests

### 2. Edge State Snapshots

- 5 new snapshot tests for edge cases
- Merge conflict indicators
- Blocked ticket UI states
- Review state visual regression

### 3. Atomic Block Handling

- `_handle_blocked` now uses single UPDATE
- Simplified `block_reason`/`last_error` in queries
- No more race conditions in scheduler tests

## Dependency Injection Pattern

**Pattern**: Use `AgentFactory` Protocol for testability.

```python
# src/kagan/agents/agent_factory.py (45 LOC)
class AgentFactory(Protocol):
    def create_agent(self, config: AgentConfig) -> Agent: ...


class DefaultAgentFactory:
    def create_agent(self, config: AgentConfig) -> Agent:
        return Agent(config)  # Real agent


# Tests inject mock factories
@pytest.fixture
def mock_agent_factory():
    factory = MagicMock(spec=AgentFactory)
    factory.create_agent.return_value = MagicMock(spec=Agent)
    return factory


# Usage in production code
class TicketRunner:
    def __init__(self, agent_factory: AgentFactory = DefaultAgentFactory()):
        self.agent_factory = agent_factory

    async def run(self):
        agent = self.agent_factory.create_agent(config)
        await agent.execute()
```

**Injected in**: TicketRunner, Scheduler, ReviewModal, PromptRefiner, PlannerScreen, KaganApp, KanbanScreen.

## Running Tests

```bash
# All tests (parallel, ~6.5s)
uv run pytest tests/

# Single test file
uv run pytest tests/e2e/test_critical_flows.py

# Single test function
uv run pytest tests/e2e/test_critical_flows.py::TestAutoTicketLifecycle::test_auto_full_lifecycle

# By marker
uv run pytest -m unit
uv run pytest -m e2e
uv run pytest -m "not slow"

# Sequential (debugging)
uv run pytest tests/ -n 0
```

## Coverage Report

```bash
uv run pytest --cov=src/kagan --cov-report=term-missing
```

**Current coverage (Phase 4)**:

- `agents/scheduler.py`: 48% (45% → 48%, +3%)
- `agents/ticket_runner.py`: 62%
- `database/manager.py`: 51%
- `ui/screens/kanban_screen.py`: 38%

**Gaps**: Error handling paths, edge cases in UI components.

## Snapshot Testing

Uses `pytest-textual-snapshot` for visual regression.

```python
@pytest.mark.snapshot
async def test_ticket_list_renders(snap_compare):
    assert await snap_compare("tests/snapshot/test_kanban_screen.py")
```

**Phase 4**: +5 snapshot tests for edge states (merge conflicts, blocks, review).

## Property-Based Testing

Uses Hypothesis for generative testing.

```python
from hypothesis import given, strategies as st


@pytest.mark.property
@given(st.text(), st.integers(min_value=0, max_value=100))
def test_ticket_title_length(title: str, max_len: int):
    truncated = truncate_title(title, max_len)
    assert len(truncated) <= max_len
```

**Configure**: `settings.register_profile("aggressive", max_examples=100, deadline=None)`

## Best Practices

1. **Use fixtures**: Avoid setup duplication
1. **Reduce max_examples**: For DB tests, use `max_examples=10`
1. **Await pilot.pause()**: In Textual tests for rendering
1. **Mark slow tests**: Use `@pytest.mark.slow` for tests >5s
1. **No blocking I/O**: Use `aiosqlite`, `asyncio.create_subprocess_exec`

## Benefits

- **Confidence**: 119 tests, 44% coverage
- **Speed**: 6.56s execution (parallel)
- **Regression prevention**: 13 snapshot tests
- **Testability**: Factory pattern enables mocking
- **Zero skipped**: All edge cases passing

## Phase 5 Goals

- Reach 50% coverage
- Add 10+ integration tests for git operations
- Expand snapshot coverage to all screens
- Property tests for state transitions
