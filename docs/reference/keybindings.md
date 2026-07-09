# Keybindings

From anywhere in OpenCode, `<leader>k` opens the board (the leader key defaults to `ctrl+x`) —
the same as running `/kagan` or the `kagan` palette command.

The keys below are active while the board route is open. Press `?` on the board for the inline
help, or run `/kagan-tutorial` to replay the guided tour.

| Key       | Action                                                                                 |
| --------- | -------------------------------------------------------------------------------------- |
| `j` / `k` | Next / previous row (card or subtask)                                                  |
| `J` / `K` | Move the selected card down / up within its column                                     |
| `g` / `G` | Jump to the first / last row in the current column                                     |
| `l` / `h` | Next / previous column (also `→` / `←`)                                                |
| `m`       | Move card to the next column (runs the gates)                                          |
| `b`       | Move card to the previous column; from Review this sends it back instead               |
| `n`       | New task (create dialog)                                                               |
| `o`       | Open the selected session                                                              |
| `Enter`   | Open the card action menu — options list only what applies, each with its own shortcut |
| `d`       | Delete the selected session                                                            |
| `a`       | Approve: triage findings, then the merge dialog                                        |
| `s`       | Send back for another iteration (Review only)                                          |
| `r`       | Retry a failed or stuck intake/review helper                                           |
| `/`       | Filter cards by title, slug, or an exact `#N` task number                              |
| `,`       | Open Kagan settings                                                                    |
| `?`       | Toggle help                                                                            |
| `q`       | Close the board                                                                        |
| `Esc`     | Dismiss: close the help overlay, else clear an active filter                           |

Moving a task backward is gated too: a started In Progress task can't drop back to Backlog (send
it back from Review, or delete it instead), and Done tasks stay put — create a follow-up task
rather than reopening one.

`J`/`K` reorder a root card only — pressing them with a subtask selected does nothing. An In
Progress card whose agent is actively working shows `● working`; one that's retrying after an
error shows `↻ retrying`. An In Progress card with neither word is quietly stalled.

In the create dialog: `Tab` moves between fields, `↑`/`↓` move outside the description box, `→` or
`Enter` opens the scope/model/base-branch pickers, `Enter` submits from the title row,
`Ctrl+Enter` submits from any field, and `Esc` cancels.

View details and archive live in the card action menu (`Enter`) rather than on dedicated keys.
**View details** opens a read-only summary of the task's title, status, intake, findings, prior
triage, reports, check/setup evidence, and diff stats.
Archiving a Done task removes it from the board — it stays reachable through OpenCode's own session
list, with no unarchive path back onto the board.
