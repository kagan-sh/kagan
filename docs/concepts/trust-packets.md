# Trust packets

A trust packet is a JSON export of everything the board knows about one task — so someone who
wasn't supervising it can judge the work from the same evidence you did, without access to your
board or worktrees.

Open a card's action menu (`Enter`) and choose **Export trust packet** or **Import trust packet**
to view one read-only.

## What's inside

| Field                       | Content                                                                 |
| --------------------------- | ----------------------------------------------------------------------- |
| `description`, `baseBranch` | What was asked, and against which branch                                |
| `intake`                    | The intake understanding and every decision you approved or overrode    |
| `findings`, `priorTriage`   | The reviewer's findings with your rulings, including earlier iterations |
| `check`, `setup`            | Deterministic command evidence: ran/skipped steps, exit codes, outputs  |
| `diffStats`                 | Per-file additions/deletions of the reviewed diff                       |
| `generation`, `approved`    | How many iterations it took, and whether you signed off                 |
| `report`                    | The agent's final report                                                |

The packet carries evidence and judgments, not code — share the branch or diff alongside it when
the reviewer needs the change itself.
