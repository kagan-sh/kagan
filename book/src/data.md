# Your data and privacy

kagan has no account, no telemetry, and no update check. Your data stays on your computer.

## Where kagan writes

- **Data folder**: `~/.kagan`, or `%USERPROFILE%\.kagan` on Windows. Set `KG_DATA_DIR` to use a different folder.
- **Socket**: the connection between `kg` and the window.
  - macOS and Linux: `$XDG_RUNTIME_DIR/kagan/kg.sock` when `XDG_RUNTIME_DIR` is set.
  - Else on macOS: `~/Library/Application Support/kagan/kg.sock`.
  - Else on Linux: `$XDG_DATA_HOME/kagan/kg.sock`, or `~/.local/share/kagan/kg.sock`.
  - Windows: a named pipe, `kagan-<your user name>`. It is not a file.
  - Set `KG_SOCKET` to use a different socket.

The data folder holds:

- Your projects, threads, and history.
- The git worktrees and Personal folders.
- Installed skills, your themes, and plugin data.

kagan saves each thread's last terminal text without encryption. A password or token that you paste can stay on disk. Delete the thread to remove its text.

## Network

kagan uses the network only to fetch a git plugin or a plugin index that you add or update. Programs in threads can use the network. A plugin can use the network only after you approve its network power.

kagan runs each known agent on your `PATH` with `--version` to show its version. The known agents are `claude`, `codex`, `gemini`, `opencode`, `cursor-agent`, `amp`, `aider`, `goose`, `qwen`, `copilot`, `grok`, and `droid`. kagan does not run other programs this way.

## Programs in threads

A program runs with your user access and gets your environment variables. These variables can include API keys. Run only programs that you trust.

The program also gets `kg`. With `kg`, it can control threads and the window. It cannot answer an approval. Only you can approve a plugin install or more plugin powers. Only you can approve a request to delete a thread.

For Claude Code and Codex, kagan adds its `kg` tools and a short note about kagan. Claude Code also gets the kagan skills. Use Settings → Agents to disable this integration.

## Plugins

A plugin gets only the powers that you approve. [Plugins](plugins.md#install) covers powers and git credentials.

## Diagnostics

**Save diagnostics** in the thread menu, or `kg diagnostics export`, writes a file for a bug report. The file has thread and plugin status. It has no terminal text, thread titles, or environment variables.

## Update kagan

1. Quit kagan.
2. Make a copy of the data folder.
3. Update kagan the way you installed it:
   - Homebrew: `brew upgrade --cask kagan`.
   - Download: download the new `.dmg` and replace `/Applications/kagan.app`. The `kg` link still works.
   - Windows, `.deb`, or AppImage: install the new file from Releases over the old one.
   - Source: run `git pull` in your `kagan` clone and build again.

## Remove kagan

1. Push or copy any work that you want to keep from worktrees and Personal folders.
2. Quit kagan.
3. Remove the app the way you installed it:
   - Homebrew: `brew uninstall --cask kagan`.
   - Download: delete `/Applications/kagan.app` and the `kg` link with `sudo rm /usr/local/bin/kg`.
   - Windows: uninstall kagan from Settings → Apps.
   - `.deb`: `sudo apt remove kagan`.
   - AppImage: delete the file.
   - Source: delete the `kagan` clone and remove it from your `PATH`.
4. Delete the data folder, `~/.kagan`.
5. On macOS and Linux, delete the folder that has `kg.sock`.
6. In each repository that you added, run `git worktree prune`.
7. If you do not need the git branches that kagan made, delete them with `git branch -D`.
