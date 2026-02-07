# Kagan

Keyboard-first Kanban TUI that turns your terminal into an AI-powered development workspace.

## Quick Start

```bash
# Install (requires Python 3.12+)
uv tool install kagan

# Launch in any git repo
cd your-project && kagan

# Create your first task: press 'n', enter details, press 'Enter' to start
```

!!! info "Requirements"
Python 3.12+, terminal (min 80x20), git repo (for worktrees), tmux (for PAIR mode)

## Essential Shortcuts

| Key         | Action                             |
| ----------- | ---------------------------------- |
| `n`         | New task                           |
| `Shift+N`   | New AUTO task (quick create)       |
| `a`         | Start agent                        |
| `Enter`     | Open session (PAIR) / watch (AUTO) |
| `g` `h`/`l` | Move task left/right               |
| `Space`     | Peek (toggle status overlay)       |
| `?` / `F1`  | Show help                          |

See the [full keyboard reference](keybindings.md) for all shortcuts.

## Two Collaboration Modes

Kagan offers two ways to work with AI on your tasks:

### AUTO Mode

Autonomous agents that work independently on tasks.

- Press ++a++ to start an agent, ++w++ to watch progress
- Agent works in background, moves to REVIEW when done
- Ideal for well-defined tasks and parallel execution

### PAIR Mode

Interactive tmux session where you work alongside your AI agent in real-time.

- Press ++enter++ on any task to open a shared terminal
- Agent has access to task context via MCP tools
- Perfect for complex problems requiring back-and-forth

!!! tip "Choosing the right mode"
Use **AUTO** for well-defined tasks with clear acceptance criteria. Use **PAIR** when you need to explore, debug, or learn alongside the AI.

Read the [Task Modes Guide](task-modes.md) for detailed state machines and workflow tips.

## Kanban Board

Classic workflow: **BACKLOG** → **IN_PROGRESS** → **REVIEW** → **DONE**

Move tasks between columns with ++g++ ++h++ / ++g++ ++l++. Track multiple workstreams with visual priority badges and agent assignments.

## AI Planner

Press ++p++ to describe your goal in natural language. The planner breaks it down into actionable tasks with acceptance criteria. Review, refine, and approve—then watch your board populate automatically.

## Review Flow

Built-in code review with AI assistance:

1. Task moves to REVIEW when agent completes work
1. View diffs (++shift+d++), inspect commits, run tests
1. Check merge readiness (Ready / At Risk / Blocked) and any preflight warnings
1. Approve (++a++) to merge or reject (++r++) with feedback
1. Auto-merge on approval (configurable)

In REVIEW, Kagan surfaces parallel in-progress tasks and their changed files to help prevent conflicts. If a merge fails, the task stays in REVIEW with a clear error and a primary resolve action. If there are no changes, the primary action becomes “Close as Exploratory” to finish without a merge.
Merges run in a dedicated merge worktree to keep your main working tree clean.

## MCP Integration

Agents access task context through MCP (Model Context Protocol):

- `get_context(task_id)` — Full task details and acceptance criteria
- `update_scratchpad(task_id, content)` — Append progress notes
- `request_review(task_id, summary)` — Submit work for review

Run the MCP server: `kagan mcp`

## Developer Tools

Kagan includes stateless utilities for developers:

```bash
kagan tools enhance "fix the login bug"                    # Enhance a prompt
kagan tools enhance "add dark mode" -t opencode            # Target specific AI tool
kagan tools enhance --file prompt.txt                      # Read from file
kagan tools enhance -f requirements.md -t claude           # Target with file input
```

## Architecture Overview

```mermaid
flowchart TB
    subgraph TUI["Kagan TUI"]
        direction LR
        B[BACKLOG]
        I[IN PROGRESS]
        R[REVIEW]
        D[DONE]
    end

    TUI --> Modes

    subgraph Modes["Collaboration Modes"]
        PAIR["PAIR Mode<br/>You + AI together"]
        AUTO["AUTO Mode<br/>AI works alone"]
    end

    Modes --> Agents

    subgraph Agents["AI Agents"]
        Claude["Claude Code"]
        OpenCode["OpenCode"]
    end

    Agents --> Code

    subgraph Code["Your Codebase"]
        Main["Main Branch"]
        Worktree["Isolated Git Branch<br/>(per task)"]
        Main <--> Worktree
    end
```

Each task gets its own isolated git branch, keeping your main branch safe until changes are reviewed and approved.

## Task Lifecycle

```mermaid
stateDiagram-v2
    [*] --> BACKLOG: Create task

    BACKLOG --> IN_PROGRESS: Start work

    state IN_PROGRESS {
        direction LR
        [*] --> Working
        Working: Work happens<br/>in isolated branch
    }

    IN_PROGRESS --> REVIEW: Work complete
    IN_PROGRESS --> BACKLOG: Blocked/Error

    state REVIEW {
        direction LR
        [*] --> Reviewing
        Reviewing: Review changes<br/>Run tests
    }

    REVIEW --> DONE: Approve & merge
    REVIEW --> BACKLOG: Reject

    DONE --> [*]
```

| Column          | AUTO Mode                            | PAIR Mode               |
| --------------- | ------------------------------------ | ----------------------- |
| **BACKLOG**     | Waiting to start                     | Waiting to start        |
| **IN_PROGRESS** | Agent works autonomously             | You collaborate with AI |
| **REVIEW**      | Auto-moves here when agent completes | You move here manually  |
| **DONE**        | Merged (auto or manual)              | Merged manually         |

!!! note "Key difference"
AUTO tasks have automatic state transitions driven by the agent. PAIR tasks are fully manual—you control when they move between columns.

## Supported AI CLIs

- **Claude Code** (Anthropic)
- **OpenCode** (open source)

Coming soon: Gemini, Codex, and more.

## Configuration

Configuration lives in the XDG config `config.toml`. Key settings:

| Setting                   | Purpose                                     |
| ------------------------- | ------------------------------------------- |
| `auto_start`              | Auto-run agents on task creation            |
| `auto_approve`            | Skip permission prompts                     |
| `auto_merge`              | Merge approved tasks automatically          |
| `require_review_approval` | Require review approval before merge        |
| `serialize_merges`        | Serialize manual merges to reduce conflicts |
| `default_worker_agent`    | Default agent (e.g., "claude")              |
| `max_concurrent_agents`   | Parallel agent limit                        |

See [Configuration](config.md) for the full reference.

## Contributing

We welcome contributions! For development setup, testing, and code style guidelines:

- [CONTRIBUTING.md on GitHub](https://github.com/aorumbayev/kagan/blob/main/CONTRIBUTING.md)
- [GitHub Issues](https://github.com/aorumbayev/kagan/issues) — Report bugs
- [GitHub Pull Requests](https://github.com/aorumbayev/kagan/pulls) — Submit code
- [GitHub Discussions](https://github.com/aorumbayev/kagan/discussions) — Ask questions
