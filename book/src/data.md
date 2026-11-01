# Your data

kagan has no account, no telemetry, and no update check. Your data stays on your computer.

## Where kagan writes

- **Data folder**: `~/.kagan`, or `%USERPROFILE%\.kagan` on Windows. To use a different folder, set `KG_DATA_DIR`.
- **Socket**: the connection that `kg` uses to talk to the window.
  - macOS and Linux: `$XDG_RUNTIME_DIR/kagan/kg.sock` when `XDG_RUNTIME_DIR` is set.
  - Else on macOS: `~/Library/Application Support/kagan/kg.sock`
  - Else on Linux: `$XDG_DATA_HOME/kagan/kg.sock`, or `~/.local/share/kagan/kg.sock`
  - Windows: a named pipe, `kagan-<your user name>`. It is not a file.
  - To use a different socket, set `KG_SOCKET`.

The data folder holds:

- `kagan.sqlite3`: your projects, your threads, and their history.
- `worktrees`: the git worktrees that kagan made for threads.
- The folders of Personal threads.
- Installed skills, your themes, and plugin data.

> [!NOTE]
> kagan saves the last terminal text of each thread in `kagan.sqlite3`, without encryption. If you paste a password or a token into a terminal, it can stay on disk. Delete the thread to remove its text.

## Network

kagan itself uses the network only to fetch a git plugin that you install or update. The programs that you run in threads can use the network. A plugin can use the network only when you approve its network power.

To show the version of each known agent, kagan runs the agents that it finds on your `PATH` with `--version`. The known agents are `claude`, `codex`, `gemini`, `opencode`, `cursor-agent`, `amp`, `aider`, `goose`, `qwen`, `copilot`, `grok`, and `droid`. kagan does not run other programs this way.

## What a program in a thread can do

A program in a thread runs with your user access. It gets your environment variables, which can include API keys. Run only programs that you trust.

The program also gets the `kg` command. With `kg`, it can start, stop, and archive threads, and operate the window. It cannot answer an approval. Only a person in the window can approve a plugin install, more plugin powers, or a thread delete.

For Claude Code and Codex, kagan adds its `kg` tools and a short note, so that the agent knows that it runs in kagan. Claude Code also gets the kagan skills. To turn this off, go to Settings → Agents.

## What a plugin can do

A plugin can do only what you approve when you install it. Each plugin runs in its own JavaScript engine, and kagan gives it only the parts of kagan that its approved powers allow. [Plugins](plugins.md#install) lists the powers. A plugin that can run `git` can use your git credentials, such as SSH keys and a GitHub token.

## Diagnostics

**Save diagnostics** in the thread menu, or `kg diagnostics export`, writes a file that you can attach to a bug report. The file has the status of each thread and each plugin. It has no terminal text, no titles, and no environment variables.

## Updates and old data

kagan cannot open a database from a pre-release build or from a different program. In that case, kagan shows the file name and stops, and it does not change the file. Move that file, and any `-wal` or `-shm` file next to it, out of the data folder. Then start kagan again.

To update kagan, run `git pull` in your `kagan` clone and build again. Before you update, quit kagan and make a copy of the data folder.

## Remove kagan

1. Push or copy any work that you want to keep from the worktrees and Personal folders.
2. Quit kagan.
3. Delete the `kagan` clone, which has the `kg` binary. Remove it from your `PATH`.
4. Delete the data folder, `~/.kagan`.
5. On macOS and Linux, delete the folder that has `kg.sock`. On Windows, there is nothing to delete.
6. In each repository that you added, run `git worktree prune`. The git branches that kagan made for threads stay. Delete them with `git branch -D` when you do not need them.
