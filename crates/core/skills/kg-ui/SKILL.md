---
name: kg-ui
description: Drive the kagan window with kg ui to test it, click through it, or take screenshots, without OS input. Use when the person asks to screenshot, click, type into, or test the kagan window, or to QA the app. It cannot approve plugin powers.
---

Read `kg guide ui`. Use `kg ui snapshot` to find element ids, then `click`, `type`, `key`, `scroll`, and `drag` (`--hold`, then `drop` or `key escape`). Use `kg ui screenshot` for PNG files. Do not use cliclick, osascript, or synthetic clicks on Approve for plugin powers. Combine with other Ops (`thread`, `theme`, `diff`, …).

Drive the instance `kg status --json` just confirmed. Snapshot that window. A second app is the wrong tree.

`kg plugin install` waits for the person. `kg ui` cannot approve powers. The Op returns `person-only approvals cannot be answered with kg ui`.

For plugin QA, do not drive the person's window. Run `kg plugin preview start <path>` and put the `KG_DATA_DIR` and `KG_SOCKET` it prints in front of each `kg ui` command [decision 274].
