# Coordination Plan

**Objective**: Devil's Advocate review of Kagan codebase - dismantle and rebuild quality logic.

**Workstreams**:

1. ARCHITECT_AGENT - Static Analysis ("Does this earn its complexity?")
1. TEST_AUDIT_AGENT - Metric Analysis ("Maximum Signal, Minimum Noise")
1. UX_AGENT - Dynamic Blind-Test ("The Blind-Fold Test")

______________________________________________________________________

# Architecture Findings (Static Analysis)

## Summary

- **Total lines analyzed**: ~19,383 lines of Python code
- **Critical issues identified**: 7
- **Files requiring refactoring**: 5
- **Estimated line reduction potential**: 15-20% (~3,000 lines)

## Critical Issues

### 1. GOD CLASS: `ui/screens/kanban/screen.py` (1,276 lines)

**Location**: `src/kagan/ui/screens/kanban/screen.py`
**Problem**: Single Responsibility Principle violation - this class handles:

- Board navigation (h/j/k/l, arrows, tab)
- Session management (tmux attach/detach)
- Agent lifecycle (spawn, monitor, stop)
- Search/filter functionality
- Review workflow (approve/reject/merge)
- Ticket CRUD operations

**Recommendation**: Extract into focused modules:

```
ui/screens/kanban/
├── screen.py          # Main screen (~200 lines)
├── navigation.py      # Focus management (~150 lines)
├── session_panel.py   # tmux integration (~200 lines)
├── agent_panel.py     # Agent status/controls (~200 lines)
├── search.py          # Search bar logic (~100 lines)
└── review.py          # Review workflow (~150 lines)
```

**Estimated savings**: 1,276 → ~1,000 lines (better organization, no duplication)

### 2. MONOLITH: `agents/scheduler.py` (957 lines)

**Location**: `src/kagan/agents/scheduler.py`
**Problem**: Handles too many concerns:

- Event queue management
- Ticket state transitions
- Agent process spawning
- Signal parsing and handling
- Iteration tracking
- Merge coordination

**Recommendation**: Extract into:

```
agents/
├── scheduler.py       # Event loop only (~200 lines)
├── transitions.py     # State machine (~150 lines)
├── spawner.py         # Process management (~150 lines)
├── signal_handler.py  # Signal parsing (already exists, expand)
└── merge_coordinator.py  # Merge logic (~100 lines)
```

### 3. CODE DUPLICATION: `database/models.py:82-108`

**Location**: Lines 82-108
**Problem**: Four nearly identical validator functions:

- `_coerce_status()`
- `_coerce_priority()`
- `_coerce_type()`
- `_coerce_agent()`

**Current** (4 functions × ~7 lines each = 28 lines):

```python
@field_validator("status", mode="before")
@classmethod
def _coerce_status(cls, v: str | Status) -> Status:
    if isinstance(v, Status):
        return v
    return Status(v)
```

**Recommended** (1 generic factory = 8 lines):

```python
def _enum_coercer(enum_cls: type[Enum]):
    def coerce(cls, v):
        return v if isinstance(v, enum_cls) else enum_cls(v)

    return field_validator(mode="before")(classmethod(coerce))


_coerce_status = _enum_coercer(Status)
_coerce_priority = _enum_coercer(Priority)
# etc.
```

**Savings**: 28 → 12 lines

### 4. REINVENTED WHEEL: `agents/worktree.py:56-70`

**Location**: Lines 56-70, `_WorktreeCache` class
**Problem**: Custom TTL cache implementation when `functools.lru_cache` or `cachetools.TTLCache` would suffice.

**Current** (15 lines):

```python
class _WorktreeCache:
    def __init__(self, ttl: float = 5.0):
        self._cache: dict[str, tuple[float, list[str]]] = {}
        self._ttl = ttl

    # ... custom expiration logic
```

**Recommended** (3 lines):

```python
from cachetools import TTLCache

_worktree_cache: TTLCache[str, list[str]] = TTLCache(maxsize=100, ttl=5.0)
```

**Savings**: 15 → 3 lines

### 5. COPY-PASTE CODE: `agents/installer.py`

**Location**: `install_claude_code()` and `install_opencode()` functions
**Problem**: ~90% identical logic for different agent installers:

- Both check if command exists
- Both run installation command
- Both verify installation
- Only differ in command names and URLs

**Recommendation**: Extract shared `_install_agent()` helper:

```python
async def _install_agent(name: str, check_cmd: str, install_cmd: list[str]) -> bool:
    if await _command_exists(check_cmd):
        return True
    # ... shared installation logic


async def install_claude_code() -> bool:
    return await _install_agent(
        "claude", "claude", ["npm", "install", "-g", "@anthropic/claude-code"]
    )
```

**Savings**: ~100 lines → ~50 lines

### 6. EMBEDDED TEMPLATES: `sessions/manager.py:293-354`

**Location**: Lines 293-354
**Problem**: 60+ line prompt template embedded in method body, making it hard to:

- Edit the template
- Test template rendering
- Reuse across different contexts

**Recommendation**: Move to `prompts/` directory as `.txt` or `.jinja2` file:

```
prompts/
├── agent_system.txt
├── ticket_context.jinja2
└── review_request.txt
```

### 7. THIN ABSTRACTIONS: `ui/modals/base.py` and `ui/screens/base.py`

**Locations**:

- `ui/modals/base.py` (24 lines)
- `ui/screens/base.py` (20 lines)

**Problem**: These base classes add abstraction without functionality:

- `ModalBase` only adds `BINDINGS = [("escape", "cancel", "Cancel")]`
- `ScreenBase` adds empty `compose()` method

**Recommendation**: Either:

1. DELETE and use Textual's built-in base classes directly
1. ADD meaningful shared behavior to justify the abstraction

## Complexity Metrics

| File                          | Lines | Complexity | Recommendation         |
| ----------------------------- | ----- | ---------- | ---------------------- |
| `ui/screens/kanban/screen.py` | 1,276 | HIGH       | Split into 5-6 modules |
| `agents/scheduler.py`         | 957   | HIGH       | Extract 4 modules      |
| `database/manager.py`         | 456   | MEDIUM     | OK, well-organized     |
| `sessions/manager.py`         | 412   | MEDIUM     | Extract templates      |
| `git_utils.py`                | 398   | MEDIUM     | OK, pure functions     |

## Action Priority

| Priority | Issue                    | Impact                   | Effort            |
| -------- | ------------------------ | ------------------------ | ----------------- |
| P1       | Split kanban/screen.py   | High (maintainability)   | High (2-3 days)   |
| P1       | Split scheduler.py       | High (testability)       | Medium (1-2 days) |
| P2       | DRY model validators     | Medium (readability)     | Low (1 hour)      |
| P2       | Replace custom cache     | Low (dependencies)       | Low (30 min)      |
| P3       | Extract templates        | Medium (maintainability) | Low (2 hours)     |
| P3       | Remove thin abstractions | Low (clarity)            | Low (30 min)      |

______________________________________________________________________

# Test Audit Findings (Efficiency Analysis)

## Summary

- **Total tests**: 144
- **Tests recommended for deletion**: 8
- **Tests recommended for merge**: 18 (into 6 parameterized tests)
- **Estimated test reduction**: 14% (20 tests -> 8 consolidated/removed)
- **All tests currently pass (144/144)**
- **Total runtime**: 6.40s (parallel execution)

## Test Distribution by Category

| Category            | Tests | % of Total | Notes                         |
| ------------------- | ----- | ---------- | ----------------------------- |
| UI Navigation (E2E) | 55    | 38%        | Slowest tests (0.4-1.0s each) |
| Agent Automation    | 51    | 35%        | Good coverage of core logic   |
| Ticket Lifecycle    | 38    | 27%        | CRUD and workflow tests       |

## Tests to DELETE (No Unique Value)

### 1. test_ui_navigation.py::TestVimNavigation - Overlapping with Arrow Navigation

- **test_j_key_moves_focus_down_within_column** (0.67s)
- **test_k_key_moves_focus_up_within_column** (0.43s)
- **test_h_key_moves_focus_to_left_column** (0.73s)
- **test_l_key_moves_focus_to_right_column** (0.58s)
- **Reason**: These tests exercise the EXACT same code paths as TestArrowNavigation tests. The vim bindings (h/j/k/l) map to the same action methods as arrow keys. Both test classes verify the same action_move_focus\_\* methods.
- **Overlap**: TestArrowNavigation::test_down/up/left/right tests already cover this
- **Recommendation**: DELETE - If vim bindings break, arrow tests would also fail

### 2. test_ui_navigation.py::TestHelpModal - Redundant trigger tests

- **test_question_mark_opens_help_modal** (0.71s)
- **test_f1_opens_help_modal** (0.61s)
- **Reason**: Both tests verify the same end state (HelpModal displayed). They only differ in keybinding trigger. One keybinding test is sufficient - the modal rendering is the same.
- **Overlap**: Both invoke action_show_help which pushes the same modal
- **Recommendation**: MERGE into single parameterized test or DELETE one

### 3. test_ui_navigation.py::TestConfirmModal::test_escape_cancels_modal

- **Reason**: Duplicates test_n_cancels_action - both verify cancellation returns False. Escape and 'n' trigger same dismiss(False) code path.
- **Overlap**: test_n_cancels_action already covers the cancellation logic

## Tests to MERGE (Redundant with Others)

### 1. TestVimNavigation + TestArrowNavigation -> Single Parameterized Test

- **Current**: 8 tests (4 vim + 4 arrow)
- **Proposed**: 1 parameterized test with 4 directions
- **Savings**: 7 tests removed

```python
@pytest.mark.parametrize(
    "key,expected_direction",
    [
        ("j", "down"),
        ("down", "down"),
        ("k", "up"),
        ("up", "up"),
        ("h", "left"),
        ("left", "left"),
        ("l", "right"),
        ("right", "right"),
    ],
)
async def test_navigation_moves_focus(key, expected_direction, e2e_app_with_tickets): ...
```

### 2. TestSignalParsing tests - Consolidate similar signal tests

- **test_parse_approve_signal_with_summary** + **test_approve_without_optional_attrs** + **test_approve_minimal**
- **Reason**: All test the same parse_signal() function with APPROVE signal. Could be a single parameterized test.
- **Savings**: 2 tests

### 3. TestScratchpadPersistence tests

- **test_scratchpad_create_and_read** + **test_scratchpad_update_replaces** + **test_scratchpad_empty_by_default**
- **Reason**: These are testing basic DB operations (create, update, default). Could be one comprehensive scratchpad test.
- **Savings**: 2 tests

### 4. TestUpdateTicket tests

- **test_update_ticket_title** + **test_update_ticket_priority** + **test_update_multiple_fields**
- **Reason**: All test same update_ticket() method with different fields. Parameterize.
- **Savings**: 2 tests

## Tests to KEEP (High Value)

### Critical Path Tests (Must Keep)

1. **TestAgentSpawning::test_auto_ticket_to_in_progress_spawns_agent** - Core feature validation
1. **TestIterationLoop::test_complete_signal_moves_to_review** - Workflow correctness
1. **TestIterationLoop::test_blocked_signal_moves_to_backlog** - Error handling path
1. **TestIterationLoop::test_max_iterations_moves_to_backlog** - Safety limit
1. **TestMergeWorkflow::test_merge_updates_ticket_state_on_success** - Happy path merge
1. **TestMergeWorkflow::test_merge_conflict_sets_blocked_state** - Conflict handling
1. **TestMergeConflictHandling::test_conflict_detection_in_preflight** - Merge safety
1. **TestWorktreeOperations::test_create_worktree** - Git isolation feature
1. **TestWorktreeOperations::test_worktree_already_exists_raises** - Error boundary
1. **TestStatusTransitions::test_status_change_callback_triggered** - Reactive system

### Well-Designed Tests (Diagnostic Value)

- All TestSignalParsing unit tests - Pure logic, fast, good edge cases
- TestRejectionHandling tests - Important user workflows
- TestAgentBlocked::test_blocked_stores_reason_in_scratchpad - Observability
- TestSchedulerQueue::test_events_processed_in_order - Concurrency correctness

## Happy Path Only Tests (Need Edge Cases)

### 1. TestCreateTicket tests

- **Current**: Only tests successful creation
- **Missing**:
  - Invalid title (empty, too long)
  - Duplicate ticket creation
  - DB connection failure handling

### 2. TestDeleteTicket tests

- **Current**: Tests successful delete
- **Missing**:
  - Delete while agent running
  - Delete ticket with active tmux session cleanup failure

### 3. TestSessionManagement tests

- **Current**: Tests happy path session creation
- **Missing**:
  - tmux not installed/not in PATH
  - Session creation when tmux server is not running
  - Orphan session cleanup

## Missing Coverage (Gaps)

### 1. Error Boundary Testing

- No tests for database connection failures
- No tests for git command failures (network down, disk full)
- No tests for agent process crashes mid-execution

### 2. Concurrency Testing

- No tests for concurrent ticket updates
- No tests for concurrent agent spawning race conditions
- No tests for scheduler shutdown during active work

### 3. Configuration Edge Cases

- No tests for missing config file
- No tests for invalid config values
- No tests for config hot-reload

### 4. UI Edge Cases

- No tests for rapid key presses
- No tests for screen resize during operations
- No tests for unicode in ticket titles

## Performance Observations

### Slow Tests (>0.7s)

UI tests are disproportionately slow due to Textual app startup:

- TestSearchFilter::test_typing_in_search_filters_tickets (0.98s)
- TestTicketDetailsModal::test_escape_closes_ticket_modal (1.02s)
- TestTicketDetailsModal::test_N_opens_auto_ticket_modal (0.93s)

**Recommendation**: Share Textual app instance across tests in same class via class-scoped fixture.

### Fast Tests (\<0.1s)

All TestSignalParsing tests run fast - they're pure unit tests without I/O.

## Mock Analysis

### Over-Mocked Tests (Risk: Testing the Mock)

1. **TestMergeOperations::test_merge_to_main_success**

   - Mocks `_fast_forward_base` which is the core merge logic
   - The test asserts on mock behavior, not real git behavior

1. **TestAutoReview tests**

   - Mock the entire agent response
   - Only tests that parse_signal works with mock data

### Under-Mocked Tests (Risk: Flaky)

1. **TestWorktreeOperations** tests
   - Uses real git operations
   - Could fail on CI with different git versions

## Recommendations Summary

1. **Merge 8 navigation tests** into 1 parameterized test (save 7 tests, ~4s)
1. **Delete duplicate help modal trigger test** (save 0.6s)
1. **Delete duplicate cancel confirmation test** (save 0.5s)
1. **Add class-scoped fixtures** for UI tests (potential 30% speedup)
1. **Add missing edge case tests** for error boundaries
1. **Reduce mock depth** in merge tests to catch real git issues

______________________________________________________________________

# UX Assessment Findings (Dynamic Blind-Test)

## Test Environment

- **Location**: `/tmp/kagan_test` (fresh git repo)
- **Terminal**: 120x40 (simulated)
- **First-time user simulation**: Yes (no prior config)

## Time-to-Interactive Metrics

| Action                          | Time    | Rating       |
| ------------------------------- | ------- | ------------ |
| App launch to first screen      | \<1s    | ✅ Excellent |
| Setup wizard to kanban board    | \<1s    | ✅ Excellent |
| Open New Ticket modal           | \<100ms | ✅ Excellent |
| Save ticket and return to board | \<100ms | ✅ Excellent |
| Open search bar                 | \<50ms  | ✅ Excellent |
| Open Help modal                 | \<100ms | ✅ Excellent |

**Verdict**: No latency issues detected. UI is responsive.

## Positive UX Patterns

### 1. Excellent Visual Affordances

- **Dimmed keybindings**: Disabled actions (View, Edit, Delete) are visually dimmed when no ticket is selected
- **Focus indicators**: Green border on focused elements, gold border on search
- **Status indicators**: "No tickets" placeholder text, ticket counts in column headers
- **Warning colors**: Yellow "◇ No description" indicator on incomplete tickets

### 2. Comprehensive Help System

- **F1/? opens tabbed help modal** with sections: Keybindings, Navigation, Concepts, Workflows
- **Contextual footer keybindings** change based on current screen/focus
- **Vim-style navigation documented** (h/j/k/l) alongside arrow keys

### 3. Progressive Disclosure

- **Plan Mode** is the first-run experience (guides users to describe features)
- **Setup wizard** on first launch with sensible defaults
- **"Getting Started"** instructions visible on empty state

### 4. Power User Features

- **Command palette** (`Ctrl+P`) for fuzzy command search
- **Vim keybindings** for navigation (h/j/k/l)
- **Tab cycling** between columns

## UX Issues Identified

### Issue #1: Missing Save Keybinding in Modal Footer (MEDIUM)

**Location**: New Ticket / Edit Ticket modal
**Reproduction**:

1. Press `n` to open New Ticket modal
1. Enter title and description
1. Look at footer for how to save

**Problem**: Footer shows only `esc Close/Cancel` and `F5 Full Editor`. No visible indication that `Ctrl+S` saves the form.

**Impact**: Users may close modal accidentally or not know how to save.

**Recommendation**: Add `^s Save` to modal footer keybindings.

### Issue #2: Modal Scrolling Hides Form Fields (MINOR)

**Location**: New Ticket modal
**Reproduction**:

1. Press `n` to open New Ticket modal
1. Tab through fields
1. Observe modal scrolls and dropdowns disappear

**Problem**: Modal height is insufficient to show all fields (Priority, Type, Agent, Status, Title, Description, Acceptance Criteria). Tabbing causes auto-scroll that hides previously visible fields.

**Impact**: Users can't see all field values at once without scrolling back.

**Recommendation**: Either:

- Increase modal height
- Use collapsible sections
- Show compact summary of non-focused fields

### Issue #3: Search Mode Captures All Keys (EXPECTED BEHAVIOR)

**Location**: Search bar
**Reproduction**:

1. Press `/` to open search
1. Press `q` (intending to quit)
1. Observe `q` is typed into search field

**Problem**: This is actually correct behavior, but may surprise users expecting `q` to quit.

**Impact**: Minimal - `Escape` correctly closes search first.

**Verdict**: NOT A BUG - expected text input behavior.

### Issue #4: CSS Loading Error in Non-Git Directories (CRITICAL)

**Location**: App startup
**Reproduction**:

1. Run `kagan` in `/tmp` (not a git repo)
1. Observe: `StylesheetError: No paths to CSS files`

**Problem**: CSS fails to load when app is run outside of a git repository, even though the app should show an error message asking user to run in a git repo.

**Impact**: Crashes instead of graceful error handling.

**Recommendation**: Ensure CSS loads before git repo check, then show user-friendly error.

## Workflow Observations

### Ticket Creation Flow

1. `n` → New Ticket modal opens ✅
1. Type title → visible in input ✅
1. Tab → moves to description ✅
1. `Ctrl+S` → saves and closes ✅
1. Ticket appears in BACKLOG ✅

**Rating**: 4/5 (would be 5/5 with visible save keybinding)

### Navigation Flow

1. Arrow keys / vim keys work ✅
1. Tab cycles columns ✅
1. Focused ticket highlighted ✅
1. `v` opens view, `e` opens edit ✅

**Rating**: 5/5

### Search Flow

1. `/` opens search bar ✅
1. Text input works ✅
1. `Escape` closes search ✅
1. Filtering is real-time (based on codebase, not tested) ✅

**Rating**: 5/5

## Accessibility Observations

| Criterion                | Status               |
| ------------------------ | -------------------- |
| Keyboard-only navigation | ✅ Full support      |
| Visual focus indicators  | ✅ Clear borders     |
| Color contrast           | ✅ Good (dark theme) |
| Screen reader support    | ⚠️ Not tested        |

## Summary

| Category        | Rating | Notes                           |
| --------------- | ------ | ------------------------------- |
| Responsiveness  | 5/5    | No perceptible lag              |
| Discoverability | 4/5    | Missing save keybinding hint    |
| Error Handling  | 3/5    | CSS crash in non-git dirs       |
| Navigation      | 5/5    | Excellent vim + arrow support   |
| Visual Design   | 5/5    | Clean, consistent, professional |

## Recommendations

1. **P1**: Add `^s Save` to modal footers
1. **P2**: Handle CSS loading before git repo validation
1. **P3**: Consider increasing modal height or using compact field view

______________________________________________________________________

# Cross-Cutting Concerns

## Pattern 1: Monolith Tendency

**Observed in**: Architecture (God classes) + Tests (slow E2E tests)

Both the `kanban/screen.py` (1,276 lines) and `scheduler.py` (957 lines) accumulate responsibilities over time. This directly impacts test speed - E2E tests are slow because they must spin up the entire monolithic app rather than testing focused components.

**Solution**: Component extraction will enable:

- Faster unit tests on extracted modules
- Reduced E2E test scope
- Better mocking boundaries

## Pattern 2: Missing Error Boundaries

**Observed in**: Tests (no error case coverage) + UX (CSS crash)

The test audit found zero tests for:

- Database connection failures
- Git command failures
- Agent process crashes

The UX audit found a CSS loading crash when run outside git repos. Both indicate a gap in defensive programming.

**Solution**:

1. Add error boundary tests for critical paths
1. Wrap initialization in try/except with graceful degradation

## Pattern 3: Duplication at Multiple Levels

**Observed in**: Architecture (validators, installer) + Tests (vim/arrow, help triggers)

Code duplication exists in:

- Model validators (4 identical functions)
- Agent installers (2 functions with 90% overlap)
- Navigation tests (vim and arrow test same logic)
- Help modal tests (two triggers for same action)

**Solution**: Apply DRY principle:

- Generic validator factory for models
- Shared installer helper
- Parameterized tests for navigation/triggers

## Pattern 4: Abstraction Without Benefit

**Observed in**: Architecture (thin base classes)

`ModalBase` and `ScreenBase` add inheritance layers without meaningful functionality. This adds cognitive overhead without reducing code.

**Solution**: Either remove thin wrappers or add genuine shared behavior.

## Priority Matrix

| Issue                    | Architecture | Tests | UX  | Combined Priority |
| ------------------------ | ------------ | ----- | --- | ----------------- |
| Split God classes        | ✅           | ✅    | -   | P1                |
| Add error boundaries     | ✅           | ✅    | ✅  | P1                |
| DRY duplication          | ✅           | ✅    | -   | P2                |
| Add save hint to modals  | -            | -     | ✅  | P2                |
| Remove thin abstractions | ✅           | -     | -   | P3                |

______________________________________________________________________

# Progress Log (w/ timestamps)

- **2026-02-03T00:00:00**: Scratchpad initialized by Orchestrator
- **2026-02-03T00:00:00**: Launching parallel workstreams
- **2026-02-03T22:XX:XX**: TEST_AUDIT_AGENT completed analysis
  - Analyzed 144 tests across 3 test files
  - Identified 8 tests for deletion, 18 for merge
  - Estimated 14% test reduction while maintaining coverage
  - Found missing error boundary and concurrency test coverage
- **2026-02-03T23:XX:XX**: ARCHITECT_AGENT completed analysis
  - Analyzed ~19,383 lines of Python code
  - Identified 7 critical issues including 2 God classes
  - Estimated 15-20% line reduction potential
  - Prioritized refactoring roadmap created
- **2026-02-03T23:XX:XX**: UX_AGENT completed blind test
  - Full TUI session with tui-mcp
  - Tested: first-run wizard, ticket creation, navigation, search, help
  - Found 2 UX issues (missing save hint, modal scrolling)
  - Found 1 critical bug (CSS crash in non-git dirs)
  - Overall UX rating: 4.4/5
- **2026-02-03T23:XX:XX**: Cross-cutting concerns synthesized
  - Identified 4 patterns spanning multiple workstreams
  - Created combined priority matrix
  - **REVIEW COMPLETE**
