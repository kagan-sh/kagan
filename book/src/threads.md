# Threads

A thread is one program in one folder, shown as a terminal in the center of the window. The program can be an agent, a shell, or any other command. In a git repository the default folder is a new worktree, so the project folder you added does not change.

## Start a thread

Click **+** next to a project, or press Cmd+N.

The New thread screen lists Claude Code and Codex even when they are not installed, other known agents on your `PATH`, your login shell, and the programs you saved. Use the arrow keys, then Enter. **Custom command…** types a different command.

```bash
kg thread run --project-id <project-id> --program claude
```

`kg project list` shows the ids. For Personal, add `--environment personal`.

A program line splits at spaces. Single or double quotes keep spaces together, for example `'/Applications/My Tool/bin/tool' --flag "a b"`. A backslash is a usual character, so a Windows path needs no change.

`kg thread paste <id> --file <path>` sends a file, up to 1 MiB. Use `-` for stdin. Paste does not press Enter.

`kg thread send <id> <text>` types that text and presses Enter, up to 1 MiB. It writes into whatever the program is reading, so a busy program or a half-typed line still receives it. What Enter does belongs to that program. The thread log gets a user message only after the write succeeds. It fails when the program is not running, when the text is empty or only control characters, or when the write fails.

## Folders

On the New thread screen, a git project offers **New worktree** and **Project folder**. New worktree is the default. Both let you pick a local branch.

- **New worktree**: a second checkout on a new branch. Your project folder does not change. It starts from the branch you pick, which begins as the current branch. kagan does not fetch, and the new branch has no upstream.
- **Project folder**: the folder you added. It stays on the current branch unless you check out a local branch or create one from a local branch. The new branch’s name is the thread title. Creating or switching waits until you commit or stash, the checkout is not in the middle of a merge, rebase, cherry-pick, or revert, HEAD is on a branch with a commit, and no other unarchived thread is using the folder.
- **Personal folder**: a new folder in the data folder, for Personal only.

A folder without a git commit always runs in the folder itself. The screen tells you.

`--environment` selects the folder:

| Value | Folder |
| --- | --- |
| `worktree` (default) | New worktree. Add `:<base>`, or `:<base>:<branch>`, to set the start point and the branch name |
| `project-default` | Project folder, on the current branch |
| `project-on:checkout:<branch>` | Project folder, after checking out that local branch |
| `project-on:new:<base>` | Project folder, on a new branch created from that local branch |
| `personal` | Personal folder |
| `reuse:<id>` | The folder of an environment that exists |

### Setup scripts

A repository can have `.kagan/setup` and `.kagan/teardown` in git. kagan runs `.kagan/setup` in a new worktree before the program starts, and `.kagan/teardown` before it removes the worktree. These scripts do not run in a project folder or a Personal folder.

> [!CAUTION]
> Before you start a thread in a repository you do not trust, click **Show script** and read it. The script can cause damage to your files.

If setup fails, the program does not start. The screen shows the error and **Retry**. Add the files the script makes to `.gitignore`.

## Stop and start again

- **Stop** (Cmd+.) ends the program. `kg thread stop <id>` does the same.
- **Start again** (Cmd+Shift+S) runs the same program in the same thread and folder. `kg thread start-again <id>` does the same.

What **Start again** continues:

- Claude Code continues its conversation. If that conversation is gone, it starts a new one, and the thread's events say so.
- Codex, when the program line is plain `codex`, continues its conversation when kagan can tell which one it was: the only conversation Codex started in that folder since the thread last started.
- If two Codex threads share a folder, or the line has other words after `codex`, Codex starts a new conversation, and the thread's events say so.
- With agent integration off in Settings, the program starts fresh.

Restarting kagan starts a program again when it was still running, and continues a Claude Code, Codex, Cursor Agent, OpenCode, Pi, or Kimi Code conversation the same way Start again does. A program you stopped stays stopped.

## When a program ends

The line under the terminal says why. The thread keeps the last screen and its scrollback after a restart.

- Exit code 0: the thread is idle.
- Another exit code, or a signal: the thread fails and shows that it needs you.
- kagan quit or crashed while the program was still running: kagan starts it again. The screen from that run is not kept. A program you had already asked to stop stays stopped, and the thread says kagan quit while it was running.

If a very large screen does not fit, the thread keeps less of it and tells you.

## Archive and delete

**Archive** takes a thread off the sidebar and keeps its history. The program stops after 10 seconds. Unarchive in those 10 seconds cancels the stop. Open **Archived** and click **Restore** to bring it back.

**Delete** removes the thread and its history. kagan asks you first.

```bash
kg thread archive <id>
kg thread unarchive <id>
kg thread delete <id>
```

After the last thread of a worktree stays archived for 10 seconds, kagan removes the worktree:

- Files that git ignores go with the folder.
- If git does not ignore some files, kagan keeps the folder and names them. Commit, delete, or ignore them, then click **Remove again**.
- The branch and its commits stay. `kg environment restore <thread-id>` makes the folder again.

kagan keeps a Personal folder until you delete its thread. kagan never removes a project folder.

## Notifications

When a thread needs you and is not on screen, kagan can show a system notification with the thread title. Settings → General → Notifications picks which events post: **All**, **Only when it asks me**, or **Off**. Or run `kg preference set notify all|asks|off`.

One thread can differ. Open the thread's menu (title bar or sidebar row), choose **Notifications**, then **Default**, **All**, **Only when it asks me**, or **Muted**. Muted threads show a small muted mark beside the title and in the sidebar. `kg thread notify <id> default|all|asks|muted` does the same.

Opening a thread clears its notification. `kg notify test` sends one test notification.

[`kg` command](cli.md) lists the thread commands and their flags.
