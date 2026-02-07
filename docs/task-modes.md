# Task Modes Guide

Kagan offers two distinct ways to work with AI on your tasks: **AUTO** mode for autonomous execution and **PAIR** mode for collaborative work.

## Quick Comparison

| Aspect             | AUTO Mode           | PAIR Mode                                             |
| ------------------ | ------------------- | ----------------------------------------------------- |
| **AI involvement** | Works independently | Works alongside you                                   |
| **Your role**      | Review results      | Active collaboration                                  |
| **Best for**       | Well-defined tasks  | Complex problems                                      |
| **Session**        | Background process  | Interactive tmux session or VS Code/Cursor launch      |
| **Start with**     | Press `a`           | Press `Enter`                                         |

## AUTO Mode: Let AI Work Independently

AUTO mode is like assigning a task to a capable assistant who works on their own and reports back when done.

### How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                        AUTO MODE FLOW                           │
└─────────────────────────────────────────────────────────────────┘

    You create          AI agent             AI signals           You review
    a task            starts work          completion           the work
        │                   │                    │                    │
        ▼                   ▼                    ▼                    ▼
   ┌─────────┐        ┌──────────┐         ┌─────────┐         ┌─────────┐
   │ BACKLOG │───────►│IN PROGRESS│────────►│ REVIEW  │────────►│  DONE   │
   └─────────┘        └──────────┘         └─────────┘         └─────────┘
                           │                                         ▲
                           │ If blocked or                           │
                           │ needs help                  Approve & merge│
                           ▼                               (manual) │
                      ┌─────────┐                                    │
                      │ BACKLOG │────────────────────────────────────┘
                      └─────────┘    (fix issue, restart)
```

### Step by Step

1. **Create a task** (press `n`)

   - Give it a clear title and description
   - Make sure task type is `AUTO` (default)

1. **Start the agent** (press `Enter` or `a`)

   - Kagan spawns an AI agent in the background
   - Agent works in an isolated git branch (your main branch stays safe)
   - Open output with `Enter`

1. **Agent works autonomously**

   - Reads your codebase
   - Makes changes
   - Runs tests
   - Commits progress

1. **Agent signals completion**

   - Task moves to REVIEW automatically
   - You get notified

1. **Review and merge**

   - Check the diff (`Shift+D`)
   - Run tests
   - Check merge readiness (Ready / At Risk / Blocked)
   - Approve (`Enter`) to merge, or reject (`r`) to send back
   - If there are no changes, use **Close as Exploratory** to finish without merging
   - Merges run in a dedicated merge worktree to keep main clean

### What Can Happen

| Agent Says | What Happens     | Your Action                   |
| ---------- | ---------------- | ----------------------------- |
| "Done!"    | Moves to REVIEW  | Review the work               |
| "Blocked"  | Moves to BACKLOG | Read the reason, help unblock |
| (error)    | Moves to BACKLOG | Check logs, fix issue         |

### Rejection Flow

When you reject work from REVIEW, you have two options:

```
REVIEW task rejected:
├── Enter (Send Back) → IN_PROGRESS (manual restart)
└── Esc (Backlog) → BACKLOG
```

| Key     | Action        | Result                                       |
| ------- | ------------- | -------------------------------------------- |
| `Enter` | **Send Back** | Task moves to IN_PROGRESS (restart manually) |
| `Esc`   | **Backlog**   | Task moves to BACKLOG for later              |

Send-back moves the task to IN_PROGRESS with your feedback appended. Start a new run manually when you're ready.

### Tips for AUTO Mode

- **Be specific** - Clear acceptance criteria help the agent succeed
- **Start small** - Break big tasks into focused tasks
- **Watch sometimes** - Press `Enter` to see current output
- **Check scratchpad** - Agent's notes show its thinking process

## PAIR Mode: Work Together with AI

PAIR mode opens an interactive terminal session where you and the AI collaborate in real-time.

### How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                        PAIR MODE FLOW                           │
└─────────────────────────────────────────────────────────────────┘

    You create          You open             You work              You finish
    a task            terminal             together              manually
        │                   │                    │                    │
        ▼                   ▼                    ▼                    ▼
   ┌─────────┐        ┌──────────┐         ┌─────────┐         ┌─────────┐
   │ BACKLOG │───────►│IN PROGRESS│────────►│ REVIEW  │────────►│  DONE   │
   └─────────┘        └──────────┘         └─────────┘         └─────────┘
        ▲                   │                    │
        │                   │                    │
        └───────────────────┴────────────────────┘
                    (all moves are manual)
```

### Step by Step

1. **Create a task** (press `n`)

   - Set task type to `PAIR`
   - Description can be more exploratory

1. **Open the session** (press `Enter`)

   - A session opens in your configured backend (`tmux`, `vscode`, or `cursor`)
   - AI agent is ready to chat
   - You're both looking at the same workspace

1. **Collaborate**

   - Ask questions, discuss approaches
   - AI can read/write code with your approval
   - You can type commands, run tests
   - It's a conversation, not a handoff

1. **Move task manually**

   - When ready, move to REVIEW (`Shift+L`)
   - Review and merge as usual (merge readiness shown in REVIEW)

### Tips for PAIR Mode

- **Think out loud** - Tell the AI what you're trying to do
- **Ask for explanations** - Great for learning and debugging
- **Take the wheel** - You can do things yourself anytime
- **Use for unknowns** - Perfect when you're not sure how to approach something

## Choosing the Right Mode

### Use AUTO When:

- Task is well-defined with clear acceptance criteria
- You want to work on something else while AI handles this
- The change is straightforward (bug fix, add feature, refactor)
- You trust the codebase patterns are established

### Use PAIR When:

- You're exploring or prototyping
- The problem is complex or ambiguous
- You want to learn while building
- You need tight control over what changes

## Configuration

These settings in the XDG config `config.toml` affect agent and merge behavior:

```toml
[general]
# Run AI review on task completion
auto_review = true

# Skip permission prompts in the planner agent
# (workers always auto-approve — they run in isolated worktrees)
auto_approve = false

# Require approved review before merge actions
require_review_approval = false

# Serialize manual merges to reduce conflicts
serialize_merges = false

# Default agent (e.g., "claude")
default_worker_agent = "claude"

# How many agents can run simultaneously
max_concurrent_agents = 1

# Default base branch for new repos
default_base_branch = "main"

# Default terminal backend for PAIR tasks (options: "tmux", "vscode", "cursor")
default_pair_terminal_backend = "tmux"
```

## Keyboard Reference

Mode-specific shortcuts at a glance:

| Key       | AUTO Mode              | PAIR Mode           |
| --------- | ---------------------- | ------------------- |
| `a`       | Start agent            | -                   |
| `s`       | Stop agent             | -                   |
| `Enter`   | Open task workspace    | Open/attach session |
| `Shift+L` | Move right (to REVIEW) | Move right          |

> **[Full Keyboard Reference ->](keybindings.md)** - Complete list of all shortcuts including the rejection modal options.

## Troubleshooting

### AUTO task keeps going back to BACKLOG

- Check the task details (`v`) or peek overlay (`space`) for the reason
- Agent may be blocked on something it can't figure out
- Try adding more context to the description
- Consider switching to PAIR mode for complex issues

### Agent seems stuck

- Press `Enter` on the task to open output
- Stop (`s`) and start a new run (`a`) if needed
- Reduce complexity of the task

### PAIR session won't open

- If using tmux backend, install tmux: `brew install tmux` or `apt install tmux`
- If using IDE backend, install VS Code or Cursor and ensure `code`/`cursor` is in PATH
- Check if another session is already open for this task

### Merge fails in REVIEW

- Kagan shows merge readiness in REVIEW before you merge
- If a merge fails, the task stays in REVIEW with the error
- Use the primary resolve action in task details to open a terminal session in the merge worktree
