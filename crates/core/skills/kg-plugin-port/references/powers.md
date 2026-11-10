# Powers to ask for

## Powers to ask for

List only what the port uses. The install screen shows each one in plain words. An update that adds one asks again.

| need | declare |
|---|---|
| read or change threads, projects, environments, questions, or the review | `powers.areas`: `threads` |
| the plugin's own database | `powers.areas`: `database` |
| a program on `PATH` | `powers.run`: `{ "program": "<name>" }`, and `powers.areas`: `process` |
| that program's full environment | `"env": "inherit"` on that program |
| HTTP | `powers.fetch`: remote hostname or `http://127.0.0.1\|localhost:<port>` (no IPv6), and `GET` and/or `POST` |
| clipboard | `powers.clipboard`: `read` and/or `write` |
| a folder under the home directory | `powers.read`: `["~/path"]`. Not the kagan data directory, `~/.ssh`, or `~/.gnupg`. |
| a folder the person picks | a `folder` setting. It is read-only. |

Refused, with no workaround: `downloads`, `powers.spawn`, `powers.serve`, and an area named `downloads`, `agents`, `notify`, or `hotkeys`.

Key-value `localStorage` needs no power. A theme needs no power. `kagan/jobs` and `kagan/host` are always there.
