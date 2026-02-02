# Kagan Architecture Reference

> **Auto-generated**: This document is periodically regenerated to reflect the current codebase state.
> **Last updated**: [TIMESTAMP]

## Quick Reference

### Project Overview

- **Framework**: Textual (Python TUI framework)
- **Python**: 3.12+
- **Package Manager**: uv
- **Database**: SQLite (aiosqlite, async)
- **Testing**: pytest with pytest-asyncio, pytest-textual-snapshot

______________________________________________________________________

## 1. Project Structure

```
src/kagan/
├── app.py                    # KaganApp - main entry point, lifecycle management
├── constants.py              # COLUMN_ORDER, STATUS_LABELS, PRIORITY_LABELS
├── config.py                 # KaganConfig Pydantic model (loaded from .kagan/config.toml)
├── keybindings.py            # ALL keybindings defined here (single source of truth)
├── theme.py                  # KAGAN_THEME, KAGAN_THEME_256 color definitions
├── terminal.py               # Terminal capability detection (truecolor support)
├── lock.py                   # Single-instance enforcement
├── git_utils.py              # Git repository initialization/detection
│
├── database/
│   ├── models.py             # Pydantic models: Ticket, TicketStatus, TicketPriority, TicketType
│   ├── manager.py            # StateManager - async database operations
│   └── queries.py            # Raw SQL queries
│
├── agents/
│   ├── scheduler.py          # Scheduler - manages agent lifecycle, spawns/stops agents
│   ├── planner.py            # Planner agent - creates tickets from natural language
│   ├── refiner.py            # Refiner agent - enhances prompts before submission
│   ├── worktree.py           # WorktreeManager - git worktree management per ticket
│   ├── prompt.py             # Prompt generation for agents
│   ├── prompt_loader.py      # Load prompts from files
│   ├── signals.py            # Agent signal parsing (<complete/>, <blocked/>, etc.)
│   ├── refinement_rules.py   # Rules for prompt refinement
│   ├── config_resolver.py    # Resolve agent configuration
│   └── installer.py          # Agent installation utilities
│
├── acp/                      # Agent Control Protocol (ACP)
│   ├── agent.py              # Agent class - spawns Claude process, manages lifecycle
│   ├── protocol.py           # ACP protocol implementation (JSON-RPC over stdio)
│   ├── jsonrpc.py            # JSON-RPC message types
│   ├── messages.py           # ACP message types
│   ├── terminals.py          # Terminal management for agents
│   ├── terminal.py           # Single terminal abstraction
│   └── buffers.py            # Output buffering
│
├── sessions/
│   ├── manager.py            # SessionManager - coordinates tmux sessions
│   └── tmux.py               # run_tmux() helper, TmuxError
│
├── mcp/                      # Model Context Protocol
│   ├── server.py             # MCP server for AI tool communication
│   └── tools.py              # MCP tool definitions
│
├── cli/
│   ├── __init__.py           # CLI entry point
│   ├── update.py             # Update command
│   └── tools.py              # CLI tools
│
├── ansi/
│   └── cleaner.py            # ANSI escape sequence cleaning
│
├── data/
│   └── builtin_agents.py     # Built-in agent definitions
│
├── styles/
│   └── kagan.tcss            # ALL CSS styles (never use DEFAULT_CSS in Python!)
│
└── ui/
    ├── __init__.py
    ├── utils/
    │   ├── animation.py      # Animation utilities
    │   └── clipboard.py      # Clipboard operations
    │
    ├── screens/
    │   ├── base.py           # Base screen class
    │   ├── welcome.py        # WelcomeScreen - first boot experience
    │   ├── planner.py        # PlannerScreen - chat-first ticket creation
    │   ├── ticket_editor.py  # TicketEditorScreen - edit ticket details
    │   ├── approval.py       # ApprovalScreen - approve agent actions
    │   ├── troubleshooting.py # TroubleshootingScreen - error diagnostics
    │   └── kanban/
    │       ├── screen.py     # KanbanScreen - main board view
    │       ├── actions.py    # Kanban action handlers
    │       └── focus.py      # Focus management for cards/columns
    │
    ├── widgets/
    │   ├── base.py           # Base widget class
    │   ├── card.py           # TicketCard - individual kanban card
    │   ├── column.py         # KanbanColumn - status column container
    │   ├── header.py         # Header widget
    │   ├── status_bar.py     # Status bar widget
    │   ├── search_bar.py     # SearchBar widget
    │   ├── empty_state.py    # Empty state placeholder
    │   ├── streaming_output.py # StreamingOutput - live agent output
    │   ├── plan_display.py   # PlanDisplay - show ticket plan
    │   ├── agent_content.py  # AgentContent - agent output rendering
    │   ├── tool_call.py      # ToolCall - render agent tool calls
    │   └── permission_prompt.py # PermissionPrompt - approve/deny actions
    │
    ├── modals/
    │   ├── __init__.py       # Modal exports
    │   ├── actions.py        # ModalAction enum
    │   ├── help.py           # HelpModal - keybinding reference
    │   ├── confirm.py        # ConfirmModal - yes/no dialogs
    │   ├── ticket_details_modal.py # TicketDetailsModal - view/edit ticket
    │   ├── diff.py           # DiffModal - show git diff
    │   ├── review.py         # ReviewModal - AI code review
    │   ├── settings.py       # SettingsModal - app configuration
    │   ├── agent_output.py   # AgentOutputModal - view agent output
    │   ├── description_editor.py # DescriptionEditorModal - edit descriptions
    │   ├── duplicate_ticket.py # DuplicateTicketModal - clone tickets
    │   ├── rejection_input.py # RejectionInputModal - rejection reason
    │   └── tmux_gateway.py   # TmuxGatewayModal - tmux session gateway
    │
    └── forms/
        └── __init__.py
```

______________________________________________________________________

## 2. Ticket/Card Model

### Enums

```python
class TicketStatus(str, Enum):
    BACKLOG = "BACKLOG"
    IN_PROGRESS = "IN_PROGRESS"
    REVIEW = "REVIEW"
    DONE = "DONE"


class TicketPriority(int, Enum):
    LOW = 0
    MEDIUM = 1
    HIGH = 2


class TicketType(str, Enum):
    AUTO = "AUTO"  # Autonomous execution via ACP scheduler
    PAIR = "PAIR"  # Pair programming via tmux session
```

### Ticket Model Fields

| Field                 | Type             | Description                 |
| --------------------- | ---------------- | --------------------------- |
| `id`                  | `str`            | 8-char hex UUID             |
| `title`               | `str`            | 1-200 chars                 |
| `description`         | `str`            | Up to 10,000 chars          |
| `status`              | `TicketStatus`   | Current kanban column       |
| `priority`            | `TicketPriority` | LOW/MEDIUM/HIGH             |
| `ticket_type`         | `TicketType`     | AUTO or PAIR                |
| `assigned_hat`        | `str \| None`    | Agent role assignment       |
| `parent_id`           | `str \| None`    | Parent ticket reference     |
| `agent_backend`       | `str \| None`    | Agent backend to use        |
| `acceptance_criteria` | `list[str]`      | List of acceptance criteria |
| `review_summary`      | `str \| None`    | Review results              |
| `checks_passed`       | `bool \| None`   | CI check status             |
| `session_active`      | `bool`           | tmux session active flag    |
| `created_at`          | `datetime`       | Creation timestamp          |
| `updated_at`          | `datetime`       | Last update timestamp       |

______________________________________________________________________

## 3. Keybinding System

**Single source of truth**: `src/kagan/keybindings.py`

### Binding Collections

| Collection                  | Used In        | Purpose                             |
| --------------------------- | -------------- | ----------------------------------- |
| `APP_BINDINGS`              | `KaganApp`     | Global app bindings (q, F1, ctrl+p) |
| `KANBAN_BINDINGS`           | `KanbanScreen` | Board navigation and actions        |
| `KANBAN_LEADER_BINDINGS`    | `KanbanScreen` | g+key sequences                     |
| `MODAL_BINDINGS` (various)  | Modals         | Modal-specific bindings             |
| `SCREEN_BINDINGS` (various) | Screens        | Screen-specific bindings            |

### Key Categories

| Category   | Keys                 | Purpose                 |
| ---------- | -------------------- | ----------------------- |
| navigation | h/j/k/l, arrows, tab | Movement                |
| primary    | n, e, v, Enter       | Main actions            |
| leader     | g+key                | Two-key sequences       |
| context    | a, s, w, D, r, m     | Status-specific actions |
| global     | q, F1, ctrl+p        | App-wide                |
| utility    | escape, ctrl+c       | Internal                |

### Adding New Keybindings

1. Add `Binding()` to appropriate collection in `keybindings.py`
1. Import collection in screen/modal
1. Implement `action_<action_name>` method
1. Help modal updates automatically via `get_key_for_action()`

______________________________________________________________________

## 4. Screen/View Architecture

### Screen Stack

```
App
└── Screen Stack (LIFO)
    ├── KanbanScreen (main)
    ├── PlannerScreen (overlay)
    ├── TicketEditorScreen (overlay)
    ├── ApprovalScreen (overlay)
    └── Modals (ModalScreen subclasses)
```

### Navigation Flow

```
WelcomeScreen (first boot)
       │
       ▼
PlannerScreen (if empty board)
       │
       ▼
KanbanScreen (main board)
    ├── n → TicketEditorScreen (new ticket)
    ├── e → TicketEditorScreen (edit ticket)
    ├── v → TicketDetailsModal
    ├── p → PlannerScreen
    ├── Enter → tmux session / agent output
    ├── D → DiffModal
    ├── r → ReviewModal
    ├── , → SettingsModal
    └── F1 → HelpModal
```

### Screen Classes

| Screen                  | File                         | Purpose                    |
| ----------------------- | ---------------------------- | -------------------------- |
| `KanbanScreen`          | `screens/kanban/screen.py`   | Main board view            |
| `PlannerScreen`         | `screens/planner.py`         | Chat-first ticket creation |
| `TicketEditorScreen`    | `screens/ticket_editor.py`   | Create/edit tickets        |
| `ApprovalScreen`        | `screens/approval.py`        | Approve agent tool calls   |
| `WelcomeScreen`         | `screens/welcome.py`         | First boot onboarding      |
| `TroubleshootingScreen` | `screens/troubleshooting.py` | Error diagnostics          |

______________________________________________________________________

## 5. Modal System

### Base Pattern

All modals inherit from `ModalScreen[T]` where `T` is the return type.

```python
class ConfirmModal(ModalScreen[bool]):
    BINDINGS = CONFIRM_BINDINGS

    def action_confirm(self) -> None:
        self.dismiss(True)

    def action_cancel(self) -> None:
        self.dismiss(False)
```

### Modal Registry

| Modal                    | Return Type      | Purpose                   |
| ------------------------ | ---------------- | ------------------------- |
| `ConfirmModal`           | `bool`           | Yes/no confirmation       |
| `HelpModal`              | `None`           | Keybinding reference      |
| `TicketDetailsModal`     | `ModalAction`    | View/edit ticket          |
| `DiffModal`              | `None`           | Show git diff             |
| `ReviewModal`            | `None`           | AI code review            |
| `SettingsModal`          | `None`           | App settings              |
| `AgentOutputModal`       | `None`           | Agent output viewer       |
| `DescriptionEditorModal` | `str \| None`    | Multiline text editor     |
| `DuplicateTicketModal`   | `Ticket \| None` | Clone ticket              |
| `RejectionInputModal`    | `str \| None`    | Rejection reason input    |
| `TmuxGatewayModal`       | `bool`           | Tmux session confirmation |

______________________________________________________________________

## 6. Ticket State Machine

### AUTO Mode (Autonomous)

```
BACKLOG → IN_PROGRESS → REVIEW → DONE
    ↑         │            │
    └─────────┴────────────┘ (rejection/error)
```

**Automatic behaviors**:

- Moving to IN_PROGRESS → Spawns agent automatically
- Agent signals `<complete/>` → Moves to REVIEW
- Agent signals `<blocked/>` → Returns to BACKLOG
- Review passes + auto_merge → Moves to DONE

### PAIR Mode (Human-Driven)

All transitions are manual. No automatic agent spawning.

### Agent Signals

| Signal                    | Effect                                  |
| ------------------------- | --------------------------------------- |
| `<continue/>`             | Stay in IN_PROGRESS, run next iteration |
| `<complete/>`             | Move to REVIEW                          |
| `<blocked reason="..."/>` | Move to BACKLOG with reason             |

______________________________________________________________________

## 7. Tmux/Session Integration

### Session Naming

Sessions follow pattern: `kagan-{ticket_id}`

### Session Lifecycle

1. User opens PAIR ticket → `TmuxGatewayModal` shown
1. User confirms → `SessionManager.create_session(ticket_id)`
1. Session created with agent attached
1. User works in tmux session
1. On exit → `session_active` flag cleared

### Key Files

- `sessions/manager.py` - `SessionManager` class
- `sessions/tmux.py` - `run_tmux()` helper
- `ui/modals/tmux_gateway.py` - Gateway modal

### Orphan Reconciliation

On app startup, `_reconcile_sessions()` kills tmux sessions for deleted tickets.

______________________________________________________________________

## 8. Agent Roles & Capabilities

| Agent Context    | read_only | Can Write | Can Run Commands |
| ---------------- | --------- | --------- | ---------------- |
| Planner          | Yes       | No        | No               |
| Refiner          | Yes       | No        | No               |
| Review Modal     | Yes       | No        | No               |
| Scheduler Review | Yes       | No        | No               |
| Worker           | No        | Yes       | Yes              |

______________________________________________________________________

## 9. Configuration

### File: `.kagan/config.toml`

```toml
[general]
auto_start = true           # Spawn agents for IN_PROGRESS on startup
auto_merge = false          # Auto-merge when review passes
auto_approve = false        # Skip approval prompts
max_iterations = 10         # Max iterations before BACKLOG
max_concurrent_agents = 2   # Concurrent agent limit
iteration_delay_seconds = 1 # Delay between iterations
default_base_branch = "main"
```

______________________________________________________________________

## 10. Database

### Async Operations

All database operations go through `StateManager` using `aiosqlite`.

### Key Methods

```python
async def get_all_tickets() -> list[Ticket]
async def get_ticket(ticket_id: str) -> Ticket | None
async def create_ticket(ticket: Ticket) -> Ticket
async def update_ticket(ticket: Ticket) -> None
async def delete_ticket(ticket_id: str) -> None
async def move_ticket(ticket_id: str, new_status: TicketStatus) -> None
async def mark_session_active(ticket_id: str, active: bool) -> None
```

### Change Notification

`StateManager` emits changes via callback → `App.ticket_changed_signal` → Screens subscribe.

______________________________________________________________________

## 11. CSS/Styling

**All styles in `src/kagan/styles/kagan.tcss`** - never use `DEFAULT_CSS` in Python!

### Key Rules

- Use `#id` selectors for unique widgets
- Use `.class` selectors for styling groups
- Use `:focus`, `:hover` pseudo-classes
- Layout via `dock`, `layout: vertical/horizontal/grid`

______________________________________________________________________

## 12. Testing

### Markers

| Marker                     | Purpose               | Mocking           |
| -------------------------- | --------------------- | ----------------- |
| `@pytest.mark.unit`        | Pure logic            | None              |
| `@pytest.mark.integration` | Component interaction | External services |
| `@pytest.mark.e2e`         | Full app tests        | Network only      |
| `@pytest.mark.snapshot`    | UI snapshots          | None              |

### Key Fixtures

- `state_manager` - Database manager
- `git_repo` - Temp git repository
- `mock_agent` - Mocked agent
- `e2e_app` - Full app for E2E tests

______________________________________________________________________

## 13. Common Patterns

### App Access from Widgets

```python
from typing import TYPE_CHECKING, cast

if TYPE_CHECKING:
    from kagan.app import KaganApp


@property
def kagan_app(self) -> KaganApp:
    return cast("KaganApp", self.app)
```

### Async Action Handlers

```python
async def action_do_something(self) -> None:
    ticket = await self.kagan_app.state_manager.get_ticket(self.ticket_id)
    # ...
```

### Worker for Background Tasks

```python
@work(exclusive=True)
async def _load_data(self) -> None:
    data = await self.fetch_data()
    self.data = data  # Triggers reactive update
```
