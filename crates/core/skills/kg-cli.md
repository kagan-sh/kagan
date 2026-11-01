---
name: kg-cli
description: Status, projects, and the binary for the running kagan app.
---

Read `kg guide` for the command and preference map, and `kg guide json`. Read `kg status --json` for thread context. Use `kg guide threads`, `kg guide projects`, `kg guide environments` and `kg guide ui` before changing work. Use --json and treat a nonzero exit as a failure.

The binary that talks to the open app is `cli` from `kg status --json`. Outside a thread, `kg` may not be on PATH. On macOS it is `/Applications/kagan.app/Contents/MacOS/kg`. An `kg mcp` process is not the app. The app is the open window, or the `--headless` process whose parent is launchd.

Leave `KG_DATA_DIR` unset. In a thread, `KG_SOCKET` is already the open app's socket; outside a thread leave it unset, or the command talks to a different app. On macOS the socket is `~/Library/Application Support/kagan/kg.sock`. If nothing answers, the command starts a second headless app. If a new `--headless` process appears, stop.

`kg status --json` returns `{"status":{"threadId","projectId","environmentId","cli","dataDir","warnings"}}`. `kg project list --json` returns `{"projects":[{"id","kind","name","path","gitRemoteUrl"}]}`. `kg program list --json` returns `{"programs":{"programs":[{"command","label","source","path","version"}],"default"}}`; pass a `command` to `kg thread run --program`.

Plugin install and powers approval are kg-plugin-authoring. Driving the open window is kg-ui.
