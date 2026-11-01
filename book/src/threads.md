# Threads

A thread is one program that runs in one folder. The program can be an agent, such as `claude` or `codex`, a shell, or any command. The terminal of the thread is where the program runs.

## Start a thread

In the window, click **+** next to a project, or press Cmd+N. The New thread screen lists:

- Claude Code and Codex, also when they are not installed.
- The other known agents that are on your `PATH`.
- Your login shell, and the programs that you used recently.

Use the arrow keys to select a program, and press Enter. To type a different command, click **Custom command…**.

From a terminal:

```bash
kg thread run --project-id <project-id> --program claude
```

`kg project list` shows the project ids. The Personal project has no git repository, so add `--environment personal` for it.

A program line splits at spaces. Single or double quotes keep spaces together, for example `'/Applications/My Tool/bin/tool' --flag "a b"`. A backslash is a usual character, so a Windows path needs no change.

## Folders

Each thread runs in a folder. On the New thread screen, a git project shows **New worktree from `<branch>`** and **Project folder**. New worktree is the default.

- **New worktree**: a second checkout of the repository, on a new branch. Your own folder does not change. The worktree starts from the current branch. kagan does not fetch, and the new branch has no upstream.
- **Project folder**: the folder that you added.
- **Personal folder**: a new folder in the data folder, for the Personal project only.

A folder without a git commit always runs in the folder itself, and the screen tells you.

From a terminal, `--environment` selects the folder:

| Value | Folder |
| --- | --- |
| `worktree` (default) | New worktree. Add `:<base>`, or `:<base>:<branch>`, to set the start point and the branch name |
| `project-default` | Project folder |
| `personal` | Personal folder |
| `reuse:<id>` | The folder of an existing environment |

### Setup scripts

A repository can have a `.kagan/setup` script and a `.kagan/teardown` script, committed in git. kagan runs `.kagan/setup` in a new worktree before the program starts. It runs `.kagan/teardown` before it removes the worktree. A project folder and a Personal folder run neither script.

> [!WARNING]
> When you click **Start**, the setup script of the repository runs on your computer. Before you start a thread in a repository that you do not trust, click **Show script** and read the script.

If the setup script fails, the program does not start. The screen shows the error and **Retry**. Add the files that the script makes to `.gitignore`.

## Stop and start again

- **Stop** (Cmd+.) ends the program. In a terminal: `kg thread stop <id>`.
- **Start again** (Cmd+Shift+S) runs the same program again, in the same thread and folder. In a terminal: `kg thread start-again <id>`.

A Claude Code thread picks up its conversation where you left it. If Claude Code no longer has that conversation, it starts a new one, and the thread's events say so. A Codex thread started as plain `codex` picks up its conversation too, when kagan can tell which one it was: the only one Codex began in that folder since the thread last started. If two Codex threads share a folder, or the line has other arguments, Codex starts a new conversation and the thread's events say so. With agent integration off in Settings, the program starts as a plain program again.

kagan never starts a program again without you, also not after a restart.

## When a program ends

The line under the terminal tells you why the program ended. The thread keeps the last screen and its scrollback, also after a restart.

- Exit code 0: the thread is idle.
- A different exit code, or a signal: the thread fails, and it shows that it needs you.
- kagan quit or crashed while the program ran: the thread fails, and no screen is saved for that run.

If a very large screen does not fit, the thread keeps less of it and tells you.

## Archive and delete

- **Archive** removes a thread from the sidebar and keeps its history. The program stops after 10 seconds. To cancel the stop, unarchive the thread in those 10 seconds. To get the thread back later, open **Archived** and click **Restore**.
- **Delete** asks you first. Then it removes the thread and its history permanently.

```bash
kg thread archive <id>
kg thread unarchive <id>
kg thread delete <id>
```

10 seconds after the last thread of a worktree is archived, kagan removes the worktree:

- Files that git ignores are deleted with the folder.
- If the folder has files that git does not ignore, kagan keeps the folder and names the files. Commit, delete, or ignore them, then click **Remove again**.
- The branch stays, with its commits. `kg environment restore <thread-id>` makes the folder again.

kagan keeps a Personal folder until you delete its thread. kagan never removes a project folder.

## Notifications

When a thread needs you and the thread is not on screen, kagan shows a system notification with the thread title. To turn this off, use the Notifications switch in Settings → Agents, or `kg preference set notify false`.

## Use threads from a terminal

| Command | What it does |
| --- | --- |
| `kg thread wait <id> --timeout-ms <ms>` | Waits until a reply ends, the program ends, or the time is up |
| `kg thread ask <id> <title> --option yes=Yes --option no=No` | Asks you a question in the Questions panel |
| `kg thread paste <id> --file <path>` | Sends the text of a file to the program, up to 1 MiB. Use `-` for stdin |
| `kg thread screen <id> --scrollback` | Prints the kept terminal text |
| `kg thread terminal new <id>` | Opens a shell in the thread's folder, as a tab in the window |
| `kg thread input <id> <text> --terminal <n>` | Types into extra terminal `n` instead of the program |
| `kg thread rename --id <id> <title>` | Sets the title. A program can do this one time. A title that you set stays |
| `kg diagnostics export` | Writes a diagnostics file. [Your data](data.md#diagnostics) tells what it has |

[`kg` command](cli.md) has all the flags.
