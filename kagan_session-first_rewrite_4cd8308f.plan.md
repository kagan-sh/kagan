---
name: Kagan Session-First Rewrite
overview: Transform Kagan from an autonomous agent orchestrator into a session-first development cockpit. Chat-first onboarding, tmux sessions per ticket, MCP for tool communication, and merge-as-review workflow. Grounded in Centaur collaboration research.
todos:
    - id: batch-a
      content: "Batch A: Schema additions + model updates + tests"
      status: pending
    - id: batch-b
      content: "Batch B: MCP server module (STDIO transport, FastMCP) + tests with mock clients"
      status: pending
    - id: batch-c
      content: "Batch C: Session manager for tmux + context injection + tests"
      status: pending
    - id: batch-d
      content: "Batch D: Welcome screen simplification + Chat-first boot + PlannerScreen + tests"
      status: pending
    - id: batch-e
      content: "Batch E: Merge-as-review flow + tests"
      status: pending
    - id: batch-f
      content: "Batch F: File cleanup (remove 12 deprecated files) + final integration tests"
      status: pending
isProject: false
---

# Kagan Session-First Rewrite Plan

A fundamental architecture shift based on human-AI collaboration research (Centaur/Cyborg models outperform full automation).

## Core Philosophy

> **"You drive, AI assists, Kagan orchestrates contexts."**

- **Chat-first**: Empty board → guided ticket creation via planner
- **Sessions**: Each ticket = tmux session + git worktree
- **MCP bridge**: AI tools call back to Kagan for state updates (STDIO transport)
- **Merge-as-review**: Quality gates run automatically; user confirms merge

---

## Code Quality Standards

All code must follow these principles:

- **Simple**: Minimal abstractions, obvious intent
- **Elegant**: Clean APIs, consistent patterns
- **Laconic**: ~150-250 LOC per module, no boilerplate
- **Maintainable**: Easy to understand, modify, delete
- **Modern**: Python 3.12+ features, type hints everywhere
- **Idiomatic**: Follow Textual, ruff, pyrefly conventions

**Style enforcement:**

```bash
uv run poe fix      # Auto-fix + format (run first!)
uv run poe typecheck  # pyrefly type checking
uv run poe check    # lint + typecheck + test
```

---

## Testing Strategy

### Principles

1. **User-facing first**: Test what users see and do, not internals
2. **Fast feedback**: Tests identify breaking changes immediately
3. **Mock external deps**: No real agents, no real tmux in CI
4. **Compact**: Each test file < 200 LOC, tests < 20 lines each

### Test Architecture

```
tests/
├── conftest.py           # Shared fixtures (in-memory DB, mock MCP, mock tmux)
├── test_database.py      # Schema + CRUD (existing, extend)
├── test_mcp_tools.py     # MCP tools with mock state manager
├── test_sessions.py      # Session manager with mock tmux
├── test_planner.py       # Planner parsing with mock ACP agent
├── test_kanban_flow.py   # Full user flows with Textual pilot
├── test_merge_flow.py    # Review → merge with mocks
└── test_snapshots.py     # Visual regression (existing)
```

### Mock Strategy

**MCP Mock** (for testing tools without real AI):

```python
@pytest.fixture
def mock_mcp_client():
    """Simulates Claude Code calling MCP tools."""
    class MockMCPClient:
        def __init__(self, server: KaganMCPServer):
            self.server = server

        async def call_tool(self, name: str, **kwargs) -> dict:
            tool = getattr(self.server, name)
            return await tool(**kwargs)

    return MockMCPClient
```

**ACP Mock** (for testing planner without real agent):

```python
@pytest.fixture
def mock_acp_agent():
    """Returns canned planner responses."""
    class MockAgent:
        def __init__(self, responses: list[str]):
            self._responses = iter(responses)

        async def send_prompt(self, prompt: str) -> str:
            return next(self._responses)

    return MockAgent
```

**Tmux Mock** (for testing sessions without real tmux):

```python
@pytest.fixture
def mock_tmux(monkeypatch):
    """Intercepts tmux subprocess calls."""
    sessions = {}

    async def fake_run_tmux(*args):
        cmd = args[0]
        if cmd == "new-session":
            name = args[args.index("-s") + 1]
            sessions[name] = {"cwd": args[args.index("-c") + 1]}
        elif cmd == "list-sessions":
            return "\n".join(sessions.keys())
        # ... etc

    monkeypatch.setattr("kagan.sessions.tmux.run_tmux", fake_run_tmux)
    return sessions
```

### Key Test Scenarios

| Scenario | What it tests | Mocks used |

|----------|---------------|------------|

| Create ticket via planner | XML parsing, DB insert | mock_acp_agent |

| Open session for ticket | Worktree + tmux + env vars | mock_tmux |

| MCP request_review | Status change, checks run | mock subprocess |

| Merge from REVIEW | Worktree merge, cleanup | mock git |

| Empty board → chat | Boot flow, screen push | in-memory DB |

### Example Tests (compact, user-facing)

```python
# tests/test_kanban_flow.py
async def test_open_session_creates_worktree_and_tmux(
    app: KaganApp, mock_tmux: dict, tmp_path: Path
):
    """User selects ticket → session created with context."""
    # Arrange
    ticket = await app.state_manager.create_ticket(
        TicketCreate(title="Add login", acceptance_criteria=["Tests pass"])
    )

    # Act
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.press("enter")
        await pilot.pause()

    # Assert
    assert f"kagan-{ticket.id}" in mock_tmux
    assert mock_tmux[f"kagan-{ticket.id}"]["env"]["KAGAN_TICKET_ID"] == ticket.id
```

```python
# tests/test_mcp_tools.py
async def test_request_review_moves_to_review(mock_state: StateManager):
    """MCP request_review moves ticket to REVIEW when checks pass."""
    ticket = await mock_state.create_ticket(
        TicketCreate(title="Feature", check_command="true")
    )
    server = KaganMCPServer(mock_state)

    result = await server.request_review(ticket.id, "Done")

    assert result["status"] == "review"
    updated = await mock_state.get_ticket(ticket.id)
    assert updated.status == TicketStatus.REVIEW
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Kagan TUI                             │
├─────────────────────────────────────────────────────────────┤
│  BOOT: .kagan/ exists?                                       │
│        → No:  WelcomeScreen (first-time setup)               │
│        → Yes: tickets == 0? → PlannerScreen                  │
│                            → KanbanScreen                    │
├─────────────────────────────────────────────────────────────┤
│  MCP Server (STDIO transport via kagan-mcp command)          │
│  - get_context(ticket_id)                                    │
│  - update_scratchpad(ticket_id, content)                     │
│  - request_review(ticket_id, summary)                        │
├─────────────────────────────────────────────────────────────┤
│  Session Manager                                             │
│  - Creates tmux sessions with context injection              │
│  - Manages worktrees                                         │
│  - Tracks session state                                      │
├─────────────────────────────────────────────────────────────┤
│  Planner Agent (ACP, on-demand)                              │
│  - Breaks requirements into tickets                          │
│  - Clarifies acceptance criteria                             │
└─────────────────────────────────────────────────────────────┘
         │
         │ User jumps to session
         ▼
    ┌─────────┐
    │  tmux   │ ← User works here with Claude Code
    │ session │ ← Claude Code spawns kagan-mcp (STDIO)
    │         │ ← kagan-mcp finds .kagan/ from cwd
    └─────────┘
```

---

## Batch A: Schema + Models

**Files:**

- [src/kagan/database/schema.sql](src/kagan/database/schema.sql)
- [src/kagan/database/models.py](src/kagan/database/models.py)
- [src/kagan/database/manager.py](src/kagan/database/manager.py)

**Additive schema changes:**

```sql
-- Ticket additions (no breaking changes)
ALTER TABLE tickets ADD COLUMN acceptance_criteria TEXT;  -- JSON: ["Tests pass", "Lint clean"]
ALTER TABLE tickets ADD COLUMN check_command TEXT;        -- e.g., "pytest && ruff check"
ALTER TABLE tickets ADD COLUMN review_summary TEXT;       -- Filled by MCP request_review
ALTER TABLE tickets ADD COLUMN checks_passed INTEGER;     -- NULL=not run, 0=fail, 1=pass
ALTER TABLE tickets ADD COLUMN session_active INTEGER DEFAULT 0;  -- Is tmux session running?
```

**New model fields:**

```python
class Ticket(BaseModel):
    # ... existing fields ...
    acceptance_criteria: list[str] = Field(default_factory=list)
    check_command: str | None = None
    review_summary: str | None = None
    checks_passed: bool | None = None
    session_active: bool = False
```

**Manager additions:**

- `mark_session_active(ticket_id, active: bool)`
- `set_review_summary(ticket_id, summary, checks_passed)`

**Tests:** Extend `test_database.py` with new field CRUD tests.

---

## Batch B: MCP Server (STDIO Transport)

**Framework:** Official MCP Python SDK (`mcp` package, 21.3k GitHub stars)

```bash
uv add mcp
```

**New module:** `src/kagan/mcp/`

```
src/kagan/mcp/
├── __init__.py     # Exports main(), KaganMCPServer
├── server.py       # FastMCP server implementation
└── tools.py        # MCP tool definitions
```

**Why STDIO (not HTTP)?**

- No port conflicts between multiple projects
- Claude Code spawns MCP server per-session
- MCP server finds `.kagan/` by traversing up from cwd
- Each project has isolated state

**Entry point (pyproject.toml):**

```toml
[project.scripts]
kagan = "kagan:run"
kagan-mcp = "kagan.mcp:main"
```

**Server implementation:**

```python
# src/kagan/mcp/server.py
from mcp.server.fastmcp import FastMCP
from kagan.database import StateManager

mcp = FastMCP("kagan")

def find_kagan_dir(start: Path) -> Path | None:
    """Find .kagan directory by traversing up."""
    current = start.resolve()
    while current != current.parent:
        if (current / ".kagan").is_dir():
            return current / ".kagan"
        current = current.parent
    return None

@mcp.tool()
async def get_context(ticket_id: str) -> dict:
    """Get ticket context for AI tools."""
    ticket = await _state().get_ticket(ticket_id)
    scratchpad = await _state().get_scratchpad(ticket_id)
    return {
        "ticket_id": ticket.id,
        "title": ticket.title,
        "description": ticket.description,
        "acceptance_criteria": ticket.acceptance_criteria,
        "scratchpad": scratchpad,
    }

@mcp.tool()
async def update_scratchpad(ticket_id: str, content: str) -> bool:
    """Append to ticket scratchpad."""
    await _state().update_scratchpad(ticket_id, content)
    return True

@mcp.tool()
async def request_review(ticket_id: str, summary: str) -> dict:
    """Mark ticket ready for review. Runs acceptance checks."""
    ticket = await _state().get_ticket(ticket_id)
    checks_passed = await _run_checks(ticket)

    if checks_passed:
        await _state().update_ticket(ticket_id, TicketUpdate(
            status=TicketStatus.REVIEW,
            review_summary=summary,
            checks_passed=True,
        ))
        return {"status": "review", "message": "Ready for merge"}
    return {"status": "failed", "message": "Checks failed"}

def main():
    """Entry point for kagan-mcp command."""
    kagan_dir = find_kagan_dir(Path.cwd())
    if not kagan_dir:
        sys.exit("Error: Not in a Kagan-managed project")

    # Initialize state manager with project's database
    global _state_manager
    _state_manager = StateManager(kagan_dir / "state.db")

    # Run STDIO server
    mcp.run(transport="stdio")
```

**User setup (one-time per machine):**

```bash
claude mcp add kagan -- kagan-mcp
```

**Tests:** `test_mcp_tools.py` with mock state manager, no real STDIO.

---

## Batch C: Session Manager (tmux) + Context Injection

**New module:** `src/kagan/sessions/`

```
src/kagan/sessions/
├── __init__.py
├── manager.py     # SessionManager class
├── tmux.py        # tmux subprocess helpers
└── context.py     # Context file generation
```

**Context Injection Layers:**

When a session is created, the agent needs to know:

1. Which ticket it's working on
2. That it's in a worktree (not main repo)
3. The acceptance criteria and rules

```
Session Context Injection:
├── 1. Environment Variables (tmux setenv)
│   KAGAN_TICKET_ID=abc123
│   KAGAN_TICKET_TITLE="Add OAuth login"
│   KAGAN_WORKTREE_PATH=/path/to/.worktrees/abc123
│   KAGAN_PROJECT_ROOT=/path/to/project
│
├── 2. Worktree/.kagan/CONTEXT.md (generated)
│   # Ticket: abc123 - Add OAuth login
│   ## Acceptance Criteria
│   - User can click "Login with Google"
│   ## Rules
│   - You are in a git worktree, not main
│   - When done: call kagan_request_review MCP tool
│
├── 3. Worktree/.claude/settings.local.json
│   { "mcpServers": { "kagan": { "command": "kagan-mcp" } } }
│
└── 4. Symlink: worktree/AGENTS.md → project/AGENTS.md
```

**SessionManager:**

```python
class SessionManager:
    """Manages tmux sessions for tickets."""

    def __init__(self, project_root: Path, state: StateManager):
        self._root = project_root
        self._state = state

    async def create_session(self, ticket: Ticket, worktree_path: Path) -> str:
        """Create tmux session with full context injection."""
        session_name = f"kagan-{ticket.id}"

        # 1. Create tmux session with environment
        await run_tmux(
            "new-session", "-d", "-s", session_name, "-c", str(worktree_path),
            "-e", f"KAGAN_TICKET_ID={ticket.id}",
            "-e", f"KAGAN_TICKET_TITLE={ticket.title}",
            "-e", f"KAGAN_WORKTREE_PATH={worktree_path}",
            "-e", f"KAGAN_PROJECT_ROOT={self._root}",
        )

        # 2. Generate CONTEXT.md in worktree
        context_md = self._generate_context(ticket)
        wt_kagan = worktree_path / ".kagan"
        wt_kagan.mkdir(exist_ok=True)
        (wt_kagan / "CONTEXT.md").write_text(context_md)

        # 3. Create local Claude settings for MCP auto-discovery
        claude_dir = worktree_path / ".claude"
        claude_dir.mkdir(exist_ok=True)
        (claude_dir / "settings.local.json").write_text(
            '{"mcpServers": {"kagan": {"command": "kagan-mcp"}}}'
        )

        # 4. Symlink AGENTS.md if exists
        agents_md = self._root / "AGENTS.md"
        wt_agents = worktree_path / "AGENTS.md"
        if agents_md.exists() and not wt_agents.exists():
            wt_agents.symlink_to(agents_md)

        await self._state.mark_session_active(ticket.id, True)
        return session_name

    def _generate_context(self, ticket: Ticket) -> str:
        """Generate CONTEXT.md content."""
        criteria = "\n".join(f"- {c}" for c in ticket.acceptance_criteria) or "- No specific criteria"
        return f"""# Ticket: {ticket.id} - {ticket.title}

## Description
{ticket.description or "No description provided."}

## Acceptance Criteria
{criteria}

## Rules
- You are in a git worktree, NOT the main repository
- Only modify files within this worktree
- Use `kagan_get_context` MCP tool to refresh ticket info
- Use `kagan_update_scratchpad` to save progress notes
- When complete: call `kagan_request_review` MCP tool

## Check Command
{ticket.check_command or "pytest && ruff check ."}
"""

    async def attach_session(self, ticket_id: str) -> None:
        """Attach to session (suspends TUI via execvp)."""
        os.execvp("tmux", ["tmux", "attach-session", "-t", f"kagan-{ticket_id}"])

    async def session_exists(self, ticket_id: str) -> bool:
        """Check if session exists."""
        output = await run_tmux("list-sessions", "-F", "#{session_name}")
        return f"kagan-{ticket_id}" in output.split("\n")

    async def kill_session(self, ticket_id: str) -> None:
        """Kill session and mark inactive."""
        await run_tmux("kill-session", "-t", f"kagan-{ticket_id}")
        await self._state.mark_session_active(ticket_id, False)
```

**Kanban integration (bindings in kanban.py):**

```python
BINDINGS = [
    # ... existing bindings ...
    Binding("enter", "open_session", "Open Session"),
    Binding("m", "merge", "Merge", show=False),  # Only in REVIEW
    Binding("d", "view_diff", "View Diff", show=False),
]

async def action_open_session(self) -> None:
    """Open tmux session for selected ticket."""
    card = self._get_focused_card()
    if not card or not card.ticket:
        return

    ticket = card.ticket

    # 1. Ensure worktree exists
    wt_mgr = self.kagan_app.worktree_manager
    wt_path = await wt_mgr.get_path(ticket.id)
    if wt_path is None:
        base = self.kagan_app.config.general.default_base_branch
        wt_path = await wt_mgr.create(ticket.id, ticket.title, base)

    # 2. Create or verify session exists
    sess_mgr = self.kagan_app.session_manager
    if not await sess_mgr.session_exists(ticket.id):
        await sess_mgr.create_session(ticket, wt_path)

    # 3. Move to IN_PROGRESS if needed
    if ticket.status == TicketStatus.BACKLOG:
        await self.kagan_app.state_manager.move_ticket(
            ticket.id, TicketStatus.IN_PROGRESS
        )

    # 4. Suspend TUI and attach to tmux (execvp replaces process)
    await sess_mgr.attach_session(ticket.id)
```

**Tests:** `test_sessions.py` with `mock_tmux` fixture that intercepts subprocess calls.

---

## Batch D: Welcome Screen Simplification + Chat-First Boot

### Welcome Screen Refinement

**Trigger:** Only on first boot (no `.kagan/` folder exists).

**Simplified design (single screen, smart defaults):**

```
┌─────────────────────────────────────────────────────────────┐
│                     ᘚᘛ KAGAN ᘚᘛ                              │
│              Your Development Cockpit                        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  AI Assistant    [Claude Code ▾]  ← Auto-detected           │
│  Base Branch     [main ▾]         ← From git                │
│                                                              │
│  ─────────────────────────────────────────────────────────  │
│                                                              │
│  ℹ For full integration, run once in your terminal:         │
│    claude mcp add kagan -- kagan-mcp                        │
│                                                              │
│    [Copy Command]  [I'll do it later]                       │
│                                                              │
├─────────────────────────────────────────────────────────────┤
│                    [Start Using Kagan]                       │
│                                                              │
│  Press Tab to customize • Escape to use defaults            │
└─────────────────────────────────────────────────────────────┘
```

**What's removed from current welcome:**

- Granular mode (worker/review/requirements agents) - not needed for session-first
- Auto-start checkbox - sessions are user-initiated

**What's kept:**

- AI assistant selection (for planner agent)
- Base branch selection (for worktrees)

**What's added:**

- MCP setup instruction (non-blocking info, not a gate)
- Copy command button

### Chat-First Boot Flow

**Modified `app.py`:**

```python
async def _initialize_app(self) -> None:
    # 1. First boot: show welcome (creates .kagan/)
    if not self.config_path.exists():
        await self.push_screen(WelcomeScreen())
        return

    # 2. Load config and initialize
    self.config = KaganConfig.load(self.config_path)
    await self._init_managers()

    # 3. Empty board: show planner chat
    tickets = await self._state_manager.get_all_tickets()
    if len(tickets) == 0:
        await self.push_screen(PlannerScreen())
    else:
        await self.push_screen(KanbanScreen())
```

### Enhanced PlannerScreen

**Refactor `chat.py` → keep as `planner.py` screen:**

```python
class PlannerScreen(KaganScreen):
    """Chat-first planner for creating tickets."""

    BINDINGS = [
        Binding("escape", "to_board", "Go to Board"),
        Binding("ctrl+c", "cancel", "Cancel"),
    ]

    def compose(self) -> ComposeResult:
        yield Static("What do you want to build?", id="planner-header")
        yield StreamingOutput(id="planner-output")
        yield Input(placeholder="Describe your feature or task...", id="planner-input")
        yield Footer()

    async def on_input_submitted(self, event: Input.Submitted) -> None:
        user_input = event.value.strip()
        if not user_input:
            return

        # Clear input, show user message
        self.query_one("#planner-input", Input).value = ""
        output = self.query_one("#planner-output", StreamingOutput)
        output.write(f"> {user_input}\n\n")

        # Send to planner agent (ACP)
        response = await self._run_planner(user_input)

        # Parse tickets from response
        tickets = parse_plan_response(response)
        if tickets:
            await self._show_ticket_preview(tickets)
```

**Planner output format:**

```xml
<plan>
  <ticket>
    <title>Implement OAuth login flow</title>
    <description>Add Google OAuth with session management</description>
    <acceptance_criteria>
      <criterion>User can click "Login with Google"</criterion>
      <criterion>Callback handles token exchange</criterion>
      <criterion>Tests pass: pytest tests/test_auth.py</criterion>
    </acceptance_criteria>
    <check_command>pytest tests/test_auth.py</check_command>
  </ticket>
</plan>
```

**Tests:** `test_planner.py` with mock ACP agent returning canned XML responses.

---

## Batch E: Merge-as-Review Flow

**Review column behavior:**

When ticket is in REVIEW (after MCP `request_review` + checks passed):

```
┌─────────────────────────────────────────────────────────────┐
│ REVIEW (1)                                                  │
├─────────────────────────────────────────────────────────────┤
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ [abc123] Add OAuth login                                │ │
│ │                                                         │ │
│ │ Summary: Implemented Google OAuth with session mgmt     │ │
│ │                                                         │ │
│ │ ✓ pytest tests/test_auth.py                             │ │
│ │ ✓ ruff check src/                                       │ │
│ │                                                         │ │
│ │ +142 -12 lines │ 5 files changed                        │ │
│ │                                                         │ │
│ │ [M] Merge  [D] View Diff  [R] Reject  [S] Re-run checks │ │
│ └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

**Merge action:**

```python
async def action_merge(self) -> None:
    """Merge ticket worktree to main branch."""
    card = self._get_focused_card()
    ticket = card.ticket

    worktree = self.kagan_app.worktree_manager

    # Merge worktree branch to main
    success = await worktree.merge_to_main(ticket.id)

    if success:
        # Clean up
        await worktree.remove(ticket.id)
        await self.kagan_app.session_manager.kill_session(ticket.id)
        await self.kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.DONE)
        self.notify(f"Merged and completed: {ticket.title}")
    else:
        self.notify("Merge conflict - resolve manually", severity="error")
```

**Config for auto-merge:**

```toml
# .kagan/config.toml
[general]
auto_merge = false  # Set true to skip merge confirmation
```

---

## Batch F: File Cleanup + Integration Tests

**This batch removes deprecated files and runs final integration tests.**

### Files to REMOVE (12 total)

```bash
# Agents - no longer needed for session-first model
rm src/kagan/agents/scheduler.py      # No auto-spawning
rm src/kagan/agents/manager.py        # Replaced by SessionManager
rm src/kagan/agents/message_bus.py    # No streaming to TUI
rm src/kagan/agents/reviewer.py       # No AI review
rm src/kagan/agents/roles.py          # No worker/reviewer roles
rm src/kagan/agents/signals.py        # No COMPLETE/BLOCKED signals
rm src/kagan/agents/prompt.py         # No iteration prompts

# UI - user is in session, not watching
rm src/kagan/ui/screens/streams.py    # No agent streaming view
rm src/kagan/ui/widgets/agent_card.py # No agent cards
rm src/kagan/ui/widgets/agent_grid.py # No agent grid

# Modals - not needed
rm src/kagan/ui/modals/agent_output.py # No agent output modal
rm src/kagan/ui/modals/permission.py   # No permission prompts
```

### Files to KEEP & MODIFY

```
src/kagan/agents/__init__.py      → Export SessionManager, keep planner
src/kagan/agents/planner.py       → Enhance XML output parsing
src/kagan/agents/prompt_loader.py → Keep for planner prompts
src/kagan/agents/worktree.py      → Add merge_to_main() method
src/kagan/ui/screens/chat.py      → Refactor to PlannerScreen
src/kagan/ui/screens/welcome.py   → Simplify per design
src/kagan/config.py               → Remove review_agent, add session config
```

### Files to ADD

```
src/kagan/mcp/__init__.py         → main() entry point for kagan-mcp
src/kagan/mcp/server.py           → FastMCP STDIO server
src/kagan/mcp/tools.py            → get_context, update_scratchpad, request_review
src/kagan/sessions/__init__.py    → SessionManager export
src/kagan/sessions/manager.py     → SessionManager class
src/kagan/sessions/tmux.py        → run_tmux() helper
src/kagan/sessions/context.py     → CONTEXT.md generation
```

### New Test Files

```
tests/test_mcp_tools.py           → MCP tools with mock StateManager
tests/test_sessions.py            → SessionManager with mock tmux
tests/test_kanban_flow.py         → Full user flows with Textual pilot
tests/test_merge_flow.py          → Review → merge → done flow
```

---

## Multi-Project MCP Handling

**How it works when running Kagan in multiple folders:**

```
Project A (/home/user/project-a/)
├── .kagan/state.db           ← Project A's database
└── .worktrees/ticket-123/    ← Session here
    └── Claude Code spawns: kagan-mcp
        └── Finds: ../../.kagan/state.db

Project B (/home/user/project-b/)  ← Completely separate!
├── .kagan/state.db           ← Project B's database
└── .worktrees/ticket-456/    ← Different session
```

**No conflicts because:**

1. **STDIO transport**: No TCP ports, Claude Code spawns `kagan-mcp` per session
2. **CWD-based discovery**: `kagan-mcp` finds `.kagan/` by traversing up from current dir
3. **Isolated databases**: Each project has its own SQLite file
4. **WAL mode**: Concurrent reads/writes handled by SQLite

**TUI ↔ MCP sync:**

- Both read/write the same `.kagan/state.db`
- TUI polls every 1-2 seconds for changes
- No real-time socket needed (SQLite is source of truth)

---

## Execution Order

```
Batch A (Schema) ─────────────────────┐
                                      ├──→ test_database.py
Batch D (Welcome + Planner) ──────────┤
                                      ├──→ test_planner.py
                                      │
Batch B (MCP Server) ─────────────────┼──→ test_mcp_tools.py
                                      │
Batch C (Session Manager) ────────────┼──→ test_sessions.py
                                      │
Batch E (Merge Flow) ─────────────────┼──→ test_merge_flow.py
                                      │
Batch F (Cleanup + Integration) ──────┘──→ test_kanban_flow.py
```

**Recommended execution:**

1. **A + D in parallel** (independent, both touch different files)
2. **B** (MCP server, testable standalone)
3. **C** (sessions, needs A for session_active field)
4. **E** (merge flow, needs A + C)
5. **F** (cleanup + full test suite)

---

## Definition of Done

Each batch complete when:

1. `uv run poe fix` passes (ruff auto-fix)
2. `uv run poe typecheck` passes (pyrefly)
3. `uv run pytest tests/ -v` passes
4. New tests cover user-facing functionality
5. Code follows style (~150-250 LOC per module)
6. No manual E2E testing required (mocks cover it)

---

## Summary

| Aspect | Approach |

|--------|----------|

| **Architecture** | Session-first, user drives, MCP bridges |

| **MCP Framework** | Official `mcp` package with FastMCP |

| **MCP Transport** | STDIO (no ports, no conflicts) |

| **Context Injection** | Env vars + CONTEXT.md + Claude settings |

| **Multi-project** | CWD-based `.kagan/` discovery |

| **Testing** | Mock ACP/MCP/tmux, test user flows |

| **Code Style** | Simple, laconic, ~200 LOC per module |
