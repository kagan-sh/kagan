# Threads

A thread is one program running in one folder. The program line is what you start, such as `claude` or `codex`. The terminal in the thread is where that program runs.

## Start and stop

In the window, pick a project and a program, then start the thread. From a terminal:

```bash
kg thread run --project-id personal --environment personal --program claude
kg thread stop <id>
```

Stop ends the program. The thread is stopping until the program has ended, then it is idle, recorded as stopped. When a program ends, its last screen and scrollback stay readable until you delete the thread or its archive grace ends, and they are kept across a restart, up to 4 MiB for the whole snapshot. Older scrollback that does not fit is left out, and the thread says so. A screen outside the geometry or that 4 MiB bound is not kept, and the thread says it was outside the geometry or the 4 MiB bound. A crash while the program was still running keeps the reason and not the screen. The thread says no output was saved for that run. A program that exits on its own with code 0 leaves the thread idle. Any other exit code, or a signal, marks the thread failed and asks for your attention. Opening the app does not start the program again. A thread left running when the app quit is marked failed. `kg diagnostics export` writes a local file. The thread actions menu has Save diagnostics. That button is also on the ended line when the program failed, was interrupted, or the screen was not kept in full. The file has no terminal contents and no environment variables. It lists each thread's id, status, and structured cause, whether a screen was kept, and each plugin's id, kind, version, and warn and error counts. Titles, end text, plugin sources, and plugin log messages are omitted.

## Environment

`--environment` picks the folder:

| Value | Folder |
| --- | --- |
| `worktree` or `worktree:<branch>` | A separate git checkout. This is the default. Only for a git project |
| `project-default` | The project folder |
| `personal` | A folder kagan creates, only for the Personal project |
| `reuse:<id>` | An existing environment |

`worktree` on Personal returns `git worktrees need a standard project`. `worktree` on a folder that is not a git repository is refused and names `project-default`. Add a folder with `kg project add <path>`.

On the new-thread screen a git project offers **Project folder** or **New worktree from `<branch>`**, and the worktree is the default. A worktree starts from the checked-out branch, or from the commit HEAD points at when HEAD is detached. A folder with no git commit runs in the folder itself, and the screen says so. If the branches cannot be read, the screen shows the error and a Retry button, and Start waits.

On the new-thread screen the list shows Claude Code and Codex (listed even when missing), other known agents when they are on the PATH, the login shell, and recent programs. Escape closes the custom command. Enter starts the selected installed program, and the arrow keys move through the list. **Custom command…** opens a Program field and an Arguments field, and shows what will run before Start. `kg program list --json` gives the same list.

The program is split on spaces. Single or double quotes keep spaces together, so `'/Applications/My Tool/bin/tool' --flag "a b"` works. A backslash is an ordinary character, so Windows paths need no escaping. An unfinished quote is an error.

A new worktree branch is created with `--no-track`. It does not track an upstream. Kagan does not fetch, and it does not set an upstream later. The only scripts are `.kagan/setup` and `.kagan/teardown`, committed in the repository, and only a git worktree runs them. They are read from the pinned base commit. On the new-thread screen, Show script opens that setup text. Files the setup script creates should be git-ignored. A project folder and a personal folder run neither.

## Start again

A stopped thread, or one the app found running when it quit, can run its program again. Choose **Start again** on its line, or press the shortcut. It starts in the same folder, in the same thread. Nothing starts it on its own.

```bash
kg thread start-again <id>
```

## Archive and delete

Archive retires a thread. The history stays. The run stops after 10 seconds. Unarchive in that window cancels the stop. The thread leaves the sidebar. Restore it from Archived.

```bash
kg thread archive <id>
kg thread unarchive <id>
kg thread delete <id>
```

Delete asks first, then removes the thread and its history for good.

A git worktree kagan created is removed 10 seconds after its last thread is archived. Git deletes ignored files with the folder. If a file git would track is still there, the folder stays, the archived row names those files, and Remove again tries once more after you commit, delete, or git-ignore them. `kg environment get` includes that reason. `kg environment remove` is the same retry. A git branch is left in place, so `kg environment restore <thread-id>` can attach the folder again once it is gone. A personal folder is kept when its thread is archived, and its files stay for `kg thread unarchive`. Deleting the thread removes the folder after you approve a question that names it. The project folder itself is never removed.

## From the terminal

`kg thread wait` returns `waiting`, `exited` (the row is idle, including after Stop), `failed`, `not-busy`, or `timeout`.

`kg thread ask` raises a question. It shows in Questions, with the thread's title. Approvals are listed first. The answer arrives as `answered`.

`kg thread paste --file` reads that file, or stdin when the path is `-`, up to 1 MiB, and sends the text.

`kg thread screen --scrollback` prints the kept lines as plain text. `--lines` takes the last rows, from 1 to 10000, and is refused without `--scrollback`.

The program may set the title once with `kg thread rename`. A later one says the title was kept. A name set in the window stays.

When a thread needs you and that thread is not on screen, kagan posts a system notification with the thread title and an empty body. `kg preference set notify false` turns that off. It stays on when the preference is unset. The Notifications switch in Settings → Agents turns it off too.

## Restart

Start the app again and the database is still there. A thread that was still starting, or still running a program, is marked failed. Saved events still load. The program is not started again.
