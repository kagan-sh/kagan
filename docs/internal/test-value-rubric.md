# Test Value Rubric

Guidelines for writing non-tautological, user-visible behavior tests.

## Core Principles

Tests exist to catch regressions in user-observable behavior, not to restate implementation details.

### What to Test

1. **User-facing behavior**: Actions that produce visible outcomes
1. **API/contract guarantees**: Documented interfaces and response shapes
1. **Real regressions**: Bugs that actually happened and could recur
1. **Boundary conditions**: Edge cases at system interfaces

### What NOT to Test

1. **Tautologies**: Tests that only verify implementation calls implementation
1. **Pass-through wiring**: Tests that mock everything and verify mocks were called
1. **Internal structure**: Tests coupled to private method signatures
1. **Already-covered paths**: Duplicates of existing test cases

## Tautology Detection

A test is tautological if:

- It mocks the thing it's testing
- It only verifies that `mock.method.assert_called_with(exact_args)`
- Changing implementation without changing behavior would break it
- It passes even when the feature is broken

### Bad Example (Tautology)

```python
def test_sync_issues_calls_adapter():
    mock_adapter = Mock()
    service = SyncService(adapter=mock_adapter)
    service.sync()
    mock_adapter.fetch_issues.assert_called_once()  # ❌ Only tests wiring
```

### Good Example (User-Visible Behavior)

```python
def test_sync_issues_creates_tasks_from_open_issues():
    adapter = FakeGitHubAdapter(
        issues=[
            Issue(number=1, title="Bug", state="OPEN"),
        ]
    )
    service = SyncService(adapter=adapter)
    result = service.sync(project_id="p1")

    assert result.stats.inserted == 1  # ✅ Observable outcome
    task = get_task_by_issue(1)
    assert task.title == "[GH-1] Bug"  # ✅ User-visible attribute
    assert task.status == TaskStatus.BACKLOG  # ✅ Documented mapping
```

## Test Categories for GitHub Plugin

### MCP Tool Contract Tests (High Value)

Test the stable external API contract:

- Tool names exist and are discoverable
- Required parameters are enforced
- Response shapes match documented schema
- Error codes are returned correctly
- Capability profile gating works

Location: `tests/plugins/github/test_mcp_github_tools_contract.py`

### Adapter Parsing Tests (Medium Value)

Test parsing of external data formats:

- gh CLI JSON output parsing
- Error message detection
- Edge cases in malformed input

Location: `tests/plugins/github/test_gh_adapter.py`

Only test parsing logic, not subprocess invocation.

### State Transition Tests (High Value)

Test deterministic board transitions:

- Issue OPEN → Task BACKLOG
- Issue CLOSED → Task DONE
- PR MERGED → Task DONE
- PR CLOSED (unmerged) → Task IN_PROGRESS

These map to user-visible board behavior.

### Lease Coordination Tests (Medium Value)

Test lease state machine:

- Acquire when free
- Block when held by other
- Takeover when stale
- Renew when held by self

Focus on observable state transitions, not internal markers.

## When to Add Tests

Add a new test when:

- [ ] Feature is user-facing (not internal refactor)
- [ ] Test would fail before fix, pass after
- [ ] No existing test covers this behavior
- [ ] Behavior is documented as a contract

Skip adding a test when:

- [ ] Change is internal implementation only
- [ ] Existing tests already cover the behavior
- [ ] Test would only verify mocks called mocks

## Test Location Guide

| Test Type              | Directory               |
| ---------------------- | ----------------------- |
| GitHub plugin contract | `tests/plugins/github/` |
| MCP tool integration   | `tests/mcp/contract/`   |
| Core service unit      | `tests/core/unit/`      |
| TUI snapshot           | `tests/tui/snapshot/`   |

Plugin tests live under `tests/plugins/` to maintain domain isolation.

## Checklist Before Adding Test

- [ ] Does this test user-visible behavior?
- [ ] Would this test fail if the feature regressed?
- [ ] Is there an existing test I should extend instead?
- [ ] Am I avoiding mocking the thing I'm testing?
- [ ] Does the test use fixtures from `tests/**/conftest.py`?

## Anti-Patterns to Avoid

### 1. Mock-Heavy Tests

If your test has more mock setup than assertions, reconsider.

### 2. Testing Framework Behavior

Don't test that Pydantic validates fields or that SQLAlchemy persists data.

### 3. Combinatorial Explosion

Don't test every parameter combination. Test representative cases.

### 4. Testing Private Methods

If you need to test a private method, it might belong in a separate module.

### 5. Snapshot Overuse

Snapshots are for UI rendering, not for API response validation.
