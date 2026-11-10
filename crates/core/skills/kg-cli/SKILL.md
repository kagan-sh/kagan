---
name: kg-cli
description: Find the running kagan app, read status, list projects and programs, and choose the right kg command. Use before acting in a thread, when kg is not found, or when a command needs the project, environment, or data folder.
---

Read `kg guide` for the command and preference map, and `kg guide json`. Read `kg status --json` for thread context. Use `kg guide threads`, `kg guide projects`, `kg guide environments` and `kg guide ui` before changing work. Use --json and treat a nonzero exit as a failure.

The binary that talks to the open app is `cli` from `kg status --json`. Outside a thread, `kg` may not be on PATH. On macOS it is `/Applications/kagan.app/Contents/MacOS/kg`. An `kg mcp` process is not the app. The app is the open window, or the `--headless` process whose parent is launchd.

Leave `KG_DATA_DIR` unset, so `kg` reaches the open app. In a thread, `KG_SOCKET` is already that app's socket. Outside a thread, leave `KG_SOCKET` unset. If nothing answers, the command starts a second app. If a new `--headless` process appears, stop it. The socket paths and the macOS binary are in `kg guide json`.

`kg status --json` returns `{"status":{"threadId","projectId","environmentId","cli","dataDir","warnings"}}`. `kg project list --json` returns `{"projects":[{"id","kind","name","path","gitRemoteUrl"}]}`. `kg program list --json` returns `{"programs":{"programs":[{"command","label","source","path","version"}],"default"}}`; pass a `command` to `kg thread run --program`.

Plugin install and powers approval are kg-plugin-authoring. Driving the open window is kg-ui. Creating and operating a thread is kg-threads.
