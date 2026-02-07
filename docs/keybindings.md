# Keyboard Shortcuts

Complete reference for all Kagan keyboard shortcuts.

## Core (All Screens)

| Key        | Action                                     |
| ---------- | ------------------------------------------ |
| `.`        | Actions palette (primary command launcher) |
| `?` / `F1` | Help                                       |
| `Esc`      | Cancel / back                              |
| `Enter`    | Primary action (confirm / open / select)   |
| `q`        | Quit                                       |

## Navigation

| Key                 | Action           |
| ------------------- | ---------------- |
| `h` / `←`           | Move focus left  |
| `l` / `→`           | Move focus right |
| `j` / `↓`           | Move focus down  |
| `k` / `↑`           | Move focus up    |
| `Tab` / `Shift+Tab` | Cycle columns    |

## Kanban Board (Core)

| Key     | Action                    |
| ------- | ------------------------- |
| `n`     | New task                  |
| `Enter` | Open session / start task |
| `/`     | Search tasks              |
| `space` | Peek overlay              |
| `e`     | Edit task                 |
| `v`     | View details              |
| `x`     | Delete task               |

## Kanban Board (Power)

| Key       | Action                           |
| --------- | -------------------------------- |
| `Shift+N` | New AUTO task                    |
| `Shift+H` | Move task left (previous column) |
| `Shift+L` | Move task right (next column)    |
| `Shift+D` | View diff (REVIEW tasks)         |
| `r`       | Open review (REVIEW tasks)       |
| `m`       | Merge (REVIEW tasks)             |
| `y`       | Duplicate task                   |
| `c`       | Copy task ID                     |
| `p`       | Open planner                     |
| `,`       | Settings                         |

## Agent Control (AUTO Tasks)

| Key | Action             |
| --- | ------------------ |
| `a` | Start agent        |
| `s` | Stop agent         |
| `w` | Watch agent output |

## Global Utilities

| Key      | Action                      |
| -------- | --------------------------- |
| `Ctrl+P` | Actions palette (secondary) |
| `Ctrl+O` | Project selector            |
| `Ctrl+R` | Repo selector               |
| `F12`    | Debug log                   |

## Planner Screen

| Key      | Action          |
| -------- | --------------- |
| `Esc`    | Return to board |
| `Ctrl+C` | Stop / cancel   |
| `F2`     | Enhance prompt  |

## Modals (Common Pattern)

| Key     | Action                        |
| ------- | ----------------------------- |
| `Enter` | Confirm / approve             |
| `Esc`   | Close / cancel                |
| `F2`    | Save / finish (edit contexts) |

### Rejection Input Modal

| Key     | Action     | Result                                                    |
| ------- | ---------- | --------------------------------------------------------- |
| `Enter` | **Retry**  | Task stays IN_PROGRESS, agent auto-restarts with feedback |
| `F2`    | **Stage**  | Task stays IN_PROGRESS but paused (restart with `a`)      |
| `Esc`   | **Shelve** | Task moves to BACKLOG for later                           |

### Permission Prompts

| Key     | Action                          |
| ------- | ------------------------------- |
| `Enter` | Allow once                      |
| `A`     | Allow always (for this session) |
| `Esc`   | Deny                            |
