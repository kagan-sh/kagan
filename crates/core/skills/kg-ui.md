---
name: kg-ui
description: Drive the kagan window with kg ui for QA and automation without OS input.
---

Read `kg guide ui`. Use `kg ui snapshot` to find element ids, then `click`, `type`, `key`, `scroll`, and `drag` (`--hold`, then `drop` or `key escape`). Use `kg ui screenshot` for PNG files. Do not use cliclick, osascript, or synthetic clicks on Approve for plugin powers. Combine with other Ops (`thread`, `theme`, `diff`, …).

Drive the instance `kg status --json` just confirmed. Snapshot that window. A second app is the wrong tree.

`kg plugin install` waits for the person. `kg ui` cannot approve powers. The Op returns `person-only approvals cannot be answered with kg ui`.
