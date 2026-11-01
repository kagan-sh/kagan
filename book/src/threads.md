# Threads

A thread runs one program in one folder. The program can be an agent, a shell, or any command.

## Start a thread

Click **+** next to a project, or press Cmd+N. The New thread screen lists:

- Claude Code and Codex. These show if your computer does not have them.
- Other known agents on your `PATH`.
- Your login shell and recent programs.

Use the arrow keys to select a program, then press Enter. Click **Custom command…** to type a different command.

From a terminal:

```bash
kg thread run --project-id <project-id> --program claude
```

`kg project list` shows the project ids. For the Personal project, add `--environment personal`.

A program line splits at spaces. Single or double quotes keep spaces together, for example `'/Applications/My Tool/bin/tool' --flag "a b"`. A backslash is a usual character, so a Windows path needs no change.

`kg thread paste <id> --file <path>` sends a file to the program, up to 1 MiB. Use `-` for stdin.

## Folders

On the New thread screen, a git project offers **New worktree from `<branch>`** and **Project folder**. New worktree is the default.

- **New worktree**: a second checkout on a new branch. Your project folder does not change. The worktree starts from the current branch. kagan does not fetch, and the new branch has no upstream.
- **Project folder**: the folder that you added.
- **Personal folder**: a new folder in the data folder, for the Personal project only.

A folder without a git commit always runs in the folder itself. The screen tells you.

From a terminal, `--environment` selects the folder:

| Value | Folder |
| --- | --- |
| `worktree` (default) | New worktree. Add `:<base>`, or `:<base>:<branch>`, to set the start point and the branch name |
| `project-default` | Project folder |
| `personal` | Personal folder |
| `reuse:<id>` | The folder of an environment that exists |

### Setup scripts

A repository can have a `.kagan/setup` script and a `.kagan/teardown` script in git. kagan runs `.kagan/setup` in a new worktree before the program starts. It runs `.kagan/teardown` before it removes the worktree. These scripts do not run in a project folder or a Personal folder.

> [!CAUTION]
> Before you start a thread in a repository that you do not trust, click **Show script** and read the script. The script can cause damage to your files.

If the setup script fails, the program does not start. The screen shows the error and **Retry**. Add the files that the script makes to `.gitignore`.

## Stop and start again

- Click **Stop**, or press Cmd+., to end the program. In a terminal, run `kg thread stop <id>`.
- Click **Start again**, or press Cmd+Shift+S, to run the same program in the same thread and folder. In a terminal, run `kg thread start-again <id>`.

Only you can start a program again. A restart of kagan does not start it.

## When a program ends

The line under the terminal tells you why the program ended. The thread keeps the last screen and its scrollback after a restart.

- Exit code 0: the thread is idle.
- A different exit code or a signal: the thread fails and shows that it needs you.
- kagan quit or crashed: the thread fails, and kagan saves no screen for that run.

If a very large screen does not fit, the thread keeps less of it and tells you.

## Archive and delete

**Archive** removes a thread from the sidebar and keeps its history. The program stops after 10 seconds. To cancel the stop, unarchive the thread in those 10 seconds. Open **Archived** and click **Restore** to get the thread back later.

**Delete** removes the thread and its history permanently. kagan asks you first.

```bash
kg thread archive <id>
kg thread unarchive <id>
kg thread delete <id>
```

After the last thread of a worktree stays archived for 10 seconds, kagan removes the worktree:

- kagan deletes files that git ignores with the folder.
- If the folder has files that git does not ignore, kagan keeps the folder and names the files. Commit, delete, or ignore them, then click **Remove again**.
- The branch and its commits stay. `kg environment restore <thread-id>` makes the folder again.

kagan keeps a Personal folder until you delete its thread. kagan never removes a project folder.

## Notifications

When a thread needs you and is not on screen, kagan shows a system notification with the thread title. To disable notifications, use the Notifications switch in Settings → Agents, or run `kg preference set notify false`.

[`kg` command](cli.md) lists the thread commands and their flags.
