# Your data and privacy

kagan has no account and no telemetry. It checks for updates only when you ask, unless you turn on the automatic check. Your data stays on your computer.

## Where kagan writes

- **Data folder**: `~/.kagan`, or `%USERPROFILE%\.kagan` on Windows. Set `KG_DATA_DIR` to use a different folder.
- **Socket**: the connection between `kg` and the window.
  - When `KG_DATA_DIR` is set and `KG_SOCKET` is not, the socket follows that folder. The folder path is made absolute. On macOS and Linux the socket is `<data folder>/kg.sock`. If that path is too long for a local socket, it is `/tmp/kagan-<id>/kg.sock`, where `<id>` is a SHA-256 of the absolute folder path, and that `/tmp` folder is private to you. On Windows the pipe is `kagan-<id>` from that same path.
  - Otherwise, macOS and Linux use `$XDG_RUNTIME_DIR/kagan/kg.sock` when `XDG_RUNTIME_DIR` is set.
  - Else on macOS: `~/Library/Application Support/kagan/kg.sock`.
  - Else on Linux: `$XDG_DATA_HOME/kagan/kg.sock`, or `~/.local/share/kagan/kg.sock`.
  - Otherwise on Windows: a named pipe, `kagan-<your user name>`. It is not a file.
  - Set `KG_SOCKET` to use a different socket. That wins over the data folder.

The data folder holds:

- Your projects, threads, and history.
- The git worktrees and Personal folders.
- Installed skills, your themes, and plugin data. A plugin script reads its own folder there as `process.dataDir`.

kagan saves each thread's last terminal text without encryption. A password or token that you paste can stay on disk. Delete the thread to remove its text.

## Network

kagan uses the network only to fetch a git plugin or a plugin index that you add or update, and to check for and download a kagan update.

- **Check for updates** (in the kagan menu, the command list, Settings → Updates, or `kg update check`) always works when you run it. It reads one signed file from the kagan releases on GitHub, over HTTPS. A program in a thread can run `kg update check` too, or turn on the automatic check.
- The automatic check runs at launch and then once a day. It is off until you turn on **Check automatically** in Settings → Updates (`kg preference set update-check true`).

Programs in threads can use the network. A plugin can use the network only after you approve its network power.

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

When a check finds a newer kagan, a line at the top of the window names the version and links to its release notes. What you can do depends on how you installed kagan:

- **Download (`.dmg`), Windows installer, or AppImage**: **Restart and install…** or **Install when kagan quits**. kagan downloads the update and checks its signature. It refuses a list or a file whose signature is missing or does not match. If kagan cannot write the folder it runs from, for example the dmg itself, it says so before it downloads: move kagan to Applications first. A restart ends every running program, so kagan first asks you and says how many programs are running. Only you can install an update. A program in a thread can run `kg update check` and `kg update status`, but it cannot install or restart.
- **Homebrew**: run `brew upgrade --cask kagan`. The line has a button to copy it. kagan does not replace a Homebrew copy itself, so Homebrew and kagan never disagree.
- **`.deb` or a source build**: the line links to the release. Install the new `.deb`, or run `git pull` in your `kagan` clone and build again.

Before a big update, you can quit kagan and copy the data folder.

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
