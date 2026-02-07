# Keyboard Shortcuts

Complete reference for all Kagan keyboard shortcuts.

## Navigation

| Key                 | Action           |
| ------------------- | ---------------- |
| `h` / `←`           | Move focus left  |
| `l` / `→`           | Move focus right |
| `j` / `↓`           | Move focus down  |
| `k` / `↑`           | Move focus up    |
| `Tab` / `Shift+Tab` | Cycle columns    |

## Tickets

| Key       | Action                                          |
| --------- | ----------------------------------------------- |
| `n`       | New ticket (opens type selection modal)         |
| `Shift+N` | New AUTO ticket directly (skips type selection) |
| `v`       | View ticket details                             |
| `e`       | Edit ticket                                     |
| `x`       | Delete ticket                                   |
| `Enter`   | Open session (PAIR) / watch agent (AUTO)        |
| `/`       | Search tickets                                  |
| `y`       | Duplicate (yank) ticket                         |
| `c`       | Copy ticket ID to clipboard                     |
| `Space`   | Toggle peek overlay (agent status/scratchpad)   |
| `f`       | Expand description                              |
| `F5`      | Open full description editor                    |

## Agent Control

| Key | Action                     |
| --- | -------------------------- |
| `a` | Start agent (AUTO tickets) |
| `s` | Stop agent                 |
| `w` | Watch agent output         |

## Leader Keys

Press `g`, then:

| Key | Action                                |
| --- | ------------------------------------- |
| `h` | Move ticket left (to previous column) |
| `l` | Move ticket right (to next column)    |
| `d` | View diff                             |
| `r` | Open review modal                     |
| `w` | Watch agent                           |

## Review

| Key       | Action            |
| --------- | ----------------- |
| `Shift+D` | View diff         |
| `r`       | Open review modal |
| `m`       | Merge ticket      |

## Global

| Key        | Action                 |
| ---------- | ---------------------- |
| `p`        | Open planner mode      |
| `,`        | Open settings          |
| `F1` / `?` | Show help              |
| `F12`      | Debug log viewer       |
| `Ctrl+P`   | Command palette        |
| `Escape`   | Deselect / close modal |
| `q`        | Quit                   |

## Modal-Specific Shortcuts

### Review Modal

| Key      | Action                    |
| -------- | ------------------------- |
| `a`      | Approve (merge changes)   |
| `r`      | Reject (provide feedback) |
| `g`      | Generate AI review        |
| `y`      | Copy content              |
| `Escape` | Close / Cancel            |

### Diff Modal

| Key      | Action                    |
| -------- | ------------------------- |
| `a`      | Approve (merge changes)   |
| `r`      | Reject (provide feedback) |
| `y`      | Copy content              |
| `Escape` | Close                     |

### Ticket Details Modal

| Key      | Action                 |
| -------- | ---------------------- |
| `e`      | Toggle edit mode       |
| `d`      | Delete ticket          |
| `f`      | Expand description     |
| `F5`     | Open full editor       |
| `y`      | Copy content           |
| `Ctrl+S` | Save changes           |
| `Escape` | Close / Cancel editing |

### Rejection Input Modal

When rejecting work, you have three options for what happens next:

| Key      | Action     | Result                                                      |
| -------- | ---------- | ----------------------------------------------------------- |
| `Enter`  | **Retry**  | Ticket stays IN_PROGRESS, agent auto-restarts with feedback |
| `Ctrl+S` | **Stage**  | Ticket stays IN_PROGRESS but paused (restart with `a`)      |
| `Escape` | **Shelve** | Ticket moves to BACKLOG for later                           |

The **Retry** action resets the iteration counter for a fresh attempt.

### Confirmation Dialogs

| Key      | Action        |
| -------- | ------------- |
| `y`      | Confirm (Yes) |
| `n`      | Cancel (No)   |
| `Escape` | Cancel        |

### Description Editor

| Key      | Action         |
| -------- | -------------- |
| `Escape` | Done editing   |
| `Ctrl+S` | Save and close |

### Agent Output Modal

| Key      | Action       |
| -------- | ------------ |
| `y`      | Copy output  |
| `c`      | Cancel agent |
| `Escape` | Close        |

### Debug Log Modal

| Key      | Action                          |
| -------- | ------------------------------- |
| `c`      | Clear all logs                  |
| `s`      | Save logs to `.kagan/debug.log` |
| `F12`    | Toggle modal (close)            |
| `Escape` | Close                           |

### Settings Modal

| Key      | Action |
| -------- | ------ |
| `Ctrl+S` | Save   |
| `Escape` | Cancel |

### Tmux Gateway Modal

| Key      | Action           |
| -------- | ---------------- |
| `Enter`  | Continue to tmux |
| `s`      | Don't show again |
| `Escape` | Cancel           |

## Planner Screen

| Key      | Action          |
| -------- | --------------- |
| `Escape` | Return to board |
| `Ctrl+C` | Cancel/Stop     |
| `Ctrl+E` | Enhance prompt  |

## Permission Prompts

When an agent requests permission:

| Key      | Action                          |
| -------- | ------------------------------- |
| `y`      | Allow once                      |
| `a`      | Allow always (for this session) |
| `n`      | Deny                            |
| `Escape` | Deny                            |
