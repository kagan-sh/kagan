# Behaviour

What kagan should do, one rule per line, for a model suite that reads this file and the `Op` signatures and does not read the implementation.

Sources, in order: [CONTEXT], [decision N], [book:page], [docs:file], [op.rs], [event.rs], [record.rs]. Help text is the CLI contract in [book:cli].

A missing id is `not-found`. A bad argument is `invalid`. Every other refusal is `failed` [op.rs].

## Questions for the person

All questions answered by the person on 2026-10-08.

## Thread

A thread is one program running in one environment, with a status (`pending`, `idle`, `starting`, `active`, `stopping`, `error`) and an append-only event log [CONTEXT]. The program line is stored on the thread. Unread means its attention is newer than its last read [CONTEXT].

### `Get`, `Metadata`, `Events`, `Interactions`

- **Allowed when**: the thread id is known [book:cli]. These reads are allowed in every status, including an archived thread [person 2026-10-08].
- **Shows**: `Get` returns the thread. The stored provider is the program line [record.rs] [op.rs]. A plugin's own thread record omits the program [docs:plugin-api]. `Metadata` that plugin's value on the thread [book:cli]. `Events` the log after `after_seq`, limited, in `asc` or `desc` order [book:cli]. The types are `created`, `started`, `exited`, `stopped`, `idle`, `waiting`, `failed`, `archived`, `unarchived`, `deleted`, `turn.started`, `turn.ended`, `message`, `tool`, `restore.failed`, `snapshot.failed`, `notice`, `answered`, `file.changed`, `file.read`, `file.predicted`, `opened`, `rewound`, and `reconnecting` [event.rs]. Types kagan no longer writes stay in the plugin event list. A live thread does not append message events [docs:architecture]. `Interactions` the pending questions; each shows its options [decision 43] [book:cli].
- **Refused when**: the thread is missing — `not-found` [op.rs].

### `List`

- **Allowed when**: always [book:cli].
- **Shows**: threads for one project when a project is set, limited, and before a timestamp [book:cli]. `--archived true` lists archived threads; `--archived false` lists threads that are not [book:cli]. An omitted `--archived` returns both [person 2026-10-08]. Hidden threads are included [person 2026-10-08].

### `Run`

- **Allowed when**: the project exists and the program resolves on the login `PATH` [op.rs].
- **Changes**: starts a thread in the chosen environment [op.rs]. The environment defaults to `worktree` [op.rs]. The status is `starting` [op.rs]. The stored provider is the full program line [op.rs]. Parent, lifecycle owner, visibility, and metadata are set here [docs:plugin-api]. The host starts that program in the thread's folder, in that thread's terminal, and keeps it running whether or not a window is open [decision 46]. Opening the app does not resume the program [docs:architecture].
- **Shows**: the new thread [book:cli].
- **Refused when**: an empty program — `program is empty` — `invalid` [op.rs]. An unfinished quote — `program has an unfinished quote` — `invalid` [op.rs]. A missing program — `program <name> was not found` — `invalid` [op.rs]. A program that cannot start — `failed` with the reason; the thread is left in `error` with a `failed` event [decision 46]. A worktree on Personal — `git worktrees need a standard project` — `invalid` [op.rs].

### `Stop`

- **Allowed when**: the thread exists and is not archived [book:cli].
- **Changes**: a thread that is still starting becomes idle. A running thread is marked `stopping`; once its program has ended, it records `stopped` and `idle` and becomes idle, with no attention. A terminal program has no turn to end [docs:architecture]. The program and what it started end at once, except a process that calls `setsid` itself; on Windows the program's Job Object ends what it started, except a process started before the program joined the job [decision 46] [decision 110]. It does not ask [decision 12].
- **Shows**: the thread leaves the running status [book:window].
- **Refused when**: a missing thread — `not-found` [op.rs]. An archived thread — `failed` [op.rs].

### `StartAgain`

- **Allowed when**: the thread exists, is not archived, its program is not running, and its folder is ready [decision 54].
- **Changes**: runs the thread's program again in the same thread and folder, at the size the window last gave it [decision 54]. A `claude` thread resumes its conversation with `--resume` and the id kagan gave it at the first start with `--session-id`. When the thread has no id, or Claude Code has no conversation with it, the agent starts a new conversation with a new id and the thread gets an info notice. A `codex` line with no arguments resumes with `codex resume <id>`: the thread's saved id, or else the one Codex conversation that began in the thread's folder since its last start, which kagan then saves. None, two, or a line with other arguments start a new conversation with an info notice. A line that already picks its conversation (for `codex`, one that names `resume` or `fork`), and every line with agent integration off, gets nothing added and no notice [decision 54] [decision 49]. Opening the app never does this on its own [decision 54].
- **Shows**: the thread, now `starting` or `active` [op.rs].
- **Refused when**: a running thread — `the program is still running` — `invalid`. An archived thread — `restore the thread before starting it again` — `invalid`. A folder that is not ready — `invalid` [op.rs].

### `Input`, `Paste`, `Report`, `Resize`, `Scroll`, `Select`, `Copy`, `Screen`

- **Allowed when**: the thread's program is running in the host [decision 46].
- **Changes**: `Input` writes text to the program's terminal and brings the view back to the bottom. `Paste` does the same, wrapped in bracketed-paste markers when the program asked for them; otherwise its newlines become carriage returns. A paste that names a path is `invalid` and sends nothing; `kg` reads `--file`, or stdin when the path is `-`, up to 1 MiB, and sends the text. Escape and the other control characters are removed from pasted text, so it cannot end a bracketed paste or type a command. `Report` sends mouse-report bytes (decimal values, 0 to 255) to the program without that scroll reset, so a scrolled view stays where it is. `Resize` sets the program's size, from 2 to 1000 columns and 1 to 500 rows. The last client to resize sets it [decision 46]. `Scroll` moves the view by lines into the history, up when positive. Each program keeps 10,000 lines of history. `Select` starts a selection at a cell (`simple`, `word`, `line` or `block`), or moves the end of the one there is (`extend`). `Screen` changes nothing.
- **Shows**: `Screen` returns the visible rows as an ANSI redraw, with the modes that change what keys send [decision 46]. With `--scrollback` it returns every kept line as plain text. `--lines` is refused without `--scrollback`; with it, the count is from 1 to 10000 and the result is that many last rows of the same text, or the whole text when there are fewer. `Copy` returns the selected text, or nothing when no text is selected; a whole line copied has no newline at its end. `Input`, `Paste`, `Report`, `Resize`, `Scroll` and `Select` return done. `WorkspaceEvent::Screen` follows a change, at most about 30 times a second [decision 46].
- **Refused when**: the thread has no running program — `not-found` [op.rs]. A size outside those limits — `invalid` [op.rs]. `--lines` without `--scrollback`, or a count outside 1 to 10000 — `invalid` [op.rs]. A settled screen is kept across a restart, at most 4 MiB for the whole snapshot [decision 62]. A crash while the program is still running leaves the reason and no screen. The thread says no output was saved for that run.

### `Terminal` (`New`, `List`, `Close`)

- **Allowed when**: the thread exists. `New` also needs it not archived and its folder ready [decision 110].
- **Changes**: `New` starts the login shell in the thread's folder with the thread's `KG_*` variables and gives it the next number; numbers are not reused while the thread lives. `Close` ends that shell and what it started: its process group on macOS and Linux, except a process that calls `setsid` itself, and its Job Object on Windows, except a process started before the shell joined the job. A shell that exits closes itself. Archive ends a thread's extra terminals when its 10-second grace ends, whether or not the program still runs; Undo within the grace keeps them. Delete and Quit end them at once. None is restored after a restart, and none writes an event or changes the thread's status or marks [decision 110].
- **Shows**: `New` returns the new terminal; `List` returns each open one with its number and the title its shell set. The terminal Ops above reach one with `--terminal <n>` [decision 110].
- **Refused when**: a missing thread, or a number that is not open — `not-found` [op.rs]. `New` on an archived thread or a folder that is not ready — `invalid`. A shell that cannot start — `failed` [decision 110].

### Terminal in the window

- The wheel and trackpad scroll the history. Shift+PageUp and Shift+PageDown page it. Output that arrives while the view is scrolled up does not move it; typing does.
- Drag selects, double-click selects a word, and triple-click selects a line. The selection is drawn in the theme's selection color.
- Copy is Cmd+C, or Ctrl+Shift+C off macOS. Paste is Cmd+V, or Ctrl+Shift+V off macOS.
- A program that asks for mouse reports (DECSET 1000, 1002 or 1003) gets clicks, drags and the wheel. It gets them in the SGR form when it asks for 1006, and in the legacy form otherwise. Shift overrides this: the press selects and the wheel scrolls in the window.
- + in the title row opens an extra terminal and shows it. While one is open, tabs over the terminal show the program first, then each extra terminal; the shown tab is filled. A click or Cmd+Alt+] and Cmd+Alt+[ switch tabs, and focus the shown terminal. × on a tab, or Cmd+Shift+W on a shown extra terminal, closes it; the tab before it shows. A terminal opened with `kg` adds a tab and does not change the shown one. Showing another thread shows its program's tab [decision 110].
- Cmd-click, or Ctrl-click off macOS, on an http or https link opens it in the system browser and does nothing else: no selection and no mouse report. A link the program set with OSC 8 opens its target; other links are read from the text, across wrapped rows [decision 112].
- When the thread's terminals printed a local link (localhost, 127.0.0.1, 0.0.0.0, [::1] or [::]), the row over the terminal shows Open host:port at its right end for the newest one. The button, Cmd+Alt+O or `kg window open-local-link` opens it [decision 112].

### `Archive`

- **Allowed when**: the thread exists [book:cli]. Allowed in every status, including a thread that is already archived [person 2026-10-08].
- **Changes**: the history stays; the run stops after 10 seconds; unarchive in that window cancels the stop [book:threads] [decision 26]. The thread leaves the sidebar [decision 41] [CONTEXT]. Archiving a lifecycle owner does the same to its dependents [decision 20] [CONTEXT]. A managed environment starts a 10 second retirement when it has no unarchived thread left [decision 26] [book:threads]. The Op never asks [decision 12] [person 2026-10-08].
- **Shows**: the thread is listed on Archived [decision 41] [book:threads]. The window asks first only when `confirm-archive` is on [person 2026-10-08].
- **Refused when**: a missing thread — `not-found` [op.rs].

### `Unarchive`

- **Allowed when**: the thread is archived [book:cli] [CONTEXT].
- **Changes**: brings it back [book:threads]. Inside the 10 second grace, the stop does not happen [book:threads]. Reuse of its environment clears retirement [docs:architecture]. It does not ask [decision 12].
- **Shows**: the thread can return to the sidebar [decision 41].
- **Refused when**: a missing thread — `not-found` [op.rs]. A thread that is not archived — `failed` [op.rs].

### `Delete`

- **Allowed when**: a person approves [decision 12] [docs:architecture].
- **Changes**: removes the thread and its history for good, and the threads that name it as lifecycle owner [CONTEXT] [decision 20]. Declining removes nothing [docs:architecture].
- **Shows**: a pending interaction on the thread named for the approval, or a standalone question when the command names no thread [docs:architecture] [book:cli]. The question shows its options [decision 43].
- **Refused when**: the person declines — `failed` [book:plugins] [op.rs]. An agent cannot answer it [decision 10]. A missing thread — `not-found` [op.rs]. A worktree whose removal was refused — `invalid`. The thread stays until Remove again [decision 28].

### `SetPinned`

- **Allowed when**: the thread exists [book:cli].
- **Changes**: pins it to the top of the sidebar, or unpins it [CONTEXT] [book:cli].
- **Refused when**: a missing thread — `not-found` [op.rs].

### `Rename`

- **Allowed when**: the thread exists, including one that is archived [book:cli].
- **Changes**: a rename in the window stores the trimmed title [book:cli]. A rename from the socket stores it once; a later one leaves the locked title as it is [record.rs]. The program, the environment, and the branch stay. It writes no event.
- **Shows**: the thread, with that title, when the title changed [book:cli].
- **Refused when**: a blank title — `invalid` [op.rs]. A locked title renamed from the socket — `invalid`, and the title stays [record.rs]. A missing thread — `not-found` [op.rs].

### `MarkRead`

- **Allowed when**: the thread exists [book:cli]. Opening the thread marks it read [decision 22].
- **Changes**: the thread is no longer unread [CONTEXT] [decision 22]. Attention itself moves on idle, on error, and when a question is raised [decision 22]. It does not append `opened`; that type is no longer written [decision 43].
- **Refused when**: a missing thread — `not-found` [op.rs].

### `SetMetadata`

- **Allowed when**: the thread exists and the caller is setting that plugin's own value [docs:plugin-api] [book:cli].
- **Changes**: stores that plugin's metadata on the thread [book:cli]. A tool with scope `tagged` is offered only on threads that carry it [docs:plugin-api].
- **Refused when**: a missing thread — `not-found` [op.rs].

### `WaitEvent`

- **Allowed when**: the thread exists [book:cli].
- **Shows**: the next event of that kind after `after_seq`, or no event when `timeout_ms` passes [docs:plugin-api] [op.rs]. The kind is one of the event types listed under `Events` [event.rs].
- **Refused when**: an unknown kind — `invalid` [op.rs]. A missing thread — `not-found` [op.rs]. A timeout is not a refusal [op.rs].

### `Wait`

- **Allowed when**: the thread exists [book:cli].
- **Shows**: `waiting` after a spinner goes quiet, `exited` when the row is idle (a clean exit or Stop), `failed` when the program failed, `not-busy` after two seconds with no spinner, or `timeout` when `timeout_ms` passes [op.rs]. `not-busy` waits out that arm. A timeout leaves the thread alone.
- **Refused when**: a missing thread — `not-found` [op.rs].

### `Ask`

- **Allowed when**: the thread exists and is not archived [op.rs].
- **Changes**: stores a question with that title and those options, for the person to answer. A second ask while that question is open does not add another [op.rs].
- **Shows**: the interaction id at once. The person's choice is an `answered` event (`title` and the option id) [event.rs]. Questions draws that title and those options from the pending row.
- **Refused when**: an empty title, or fewer than two options — `invalid` [op.rs]. Nothing is stored. A missing thread — `not-found` [op.rs]. An archived thread — `invalid` [op.rs]. `threads.answer` on this question fails with "a person answers this", and the card stays [docs:plugin-api].

### `Tools`

- **Allowed when**: the thread exists [book:cli].
- **Shows**: the tools offered to that thread, and the plugin instructions [book:cli]. A plugin's instructions go out as MCP server instructions; each tool description still stands alone [docs:plugin-api]. With no plugin host the list is empty. That is the contract before plugins load [docs:architecture] [person 2026-10-08].
- **Refused when**: a missing thread — `not-found` [op.rs].

### `CallTool`

- **Allowed when**: the named tool is offered to that thread [book:cli]. Its input is checked against the tool's schema before the plugin runs [docs:plugin-api].
- **Shows**: the tool's text, and whether that reply is an error [book:cli]. A tool or CLI job that runs past 120 seconds fails with an error that names the plugin [docs:plugin-api].
- **Refused when**: an unknown tool, including when no plugin host is installed — `not-found` [docs:architecture] [op.rs]. A schema miss — `invalid` [docs:plugin-api] [op.rs]. That empty host is the contract before plugins load [person 2026-10-08].

### `Details`

- **Allowed when**: the thread exists and has a folder [decision 111].
- **Changes**: nothing. It never fetches, never writes to the repository, and runs no `git status` [decision 111].
- **Shows**: the folder, the environment kind, the program line, and, in a git folder, the branch (none on a detached HEAD), the base branch with how many commits the branch is ahead of and behind it (none when the repository has no base branch), and the last five commits with short revs, newest first. Outside git, `git` is none [decision 111].
- **Refused when**: a missing thread — `not-found`. A thread with no folder — `invalid` [op.rs].

### `Links`

- **Allowed when**: the thread exists [decision 112].
- **Changes**: nothing.
- **Shows**: every http and https link the thread's terminals keep, as plain text, the program's terminal first and then each extra terminal in number order, each link once at the last place it was printed [decision 112].
- **Refused when**: a missing thread — `not-found` [op.rs].

### `Search`

- **Allowed when**: always [op.rs].
- **Shows**: threads whose title, or whose kept terminal lines, hold the text, ignoring case: threads that are not archived first, then archived ones when `--archived` is given, each marked `archived`. Hidden threads are left out. Each has its project's name and, when a kept line matched, the last such line, cut around the match, and the lines to scroll back to show it. At most `--limit`, 50 when left out [decision 107]. Kept lines are the running program's scrollback, or the screen kept after it ended; an archived thread's screen is released when its grace ends, so then only its title is searched [decision 107] [decision 26].
- **Refused when**: blank text, text longer than 256 characters, or a limit outside 1 to 200 — `invalid` [op.rs].

## Diagnostics

`kg diagnostics export` writes one new local file. Omit `--path` and it goes under the data directory, in `diagnostics/`. A given path is refused when the parent folder is missing or the file already exists.

- **Changes**: creates that file. It names the kagan version, the schema version, and the data directory. Each thread gets its id, status, and structured cause, and whether a screen was kept. Each plugin gets its id, kind, version, and warn and error counts. Titles, end text, plugin sources, and plugin log messages are omitted. The file says a program that was still running when kagan quit has no saved screen.
- **Refused when**: the file already exists, or the folder for `--path` does not exist — `invalid` [op.rs]. The thread actions menu has the same export. The ended line has that button when the program failed, was interrupted, or its screen was not kept in full. A clean stop or an exit 0 does not. There is no default shortcut.

## Project

A project is a named body of work, usually one repository, with one folder on this machine. Its kind is `standard` or `personal` [CONTEXT]. The first launch creates the Personal project; its id is `personal` [book:start].

### `List`, `Get`

- **Allowed when**: `Get` needs a project id [book:cli]. `List` always [book:cli].
- **Shows**: the projects, or one project [book:cli]. Personal is last in the sidebar [docs:look].
- **Refused when**: a missing id on `Get` — `not-found` [op.rs].

### `Add`

- **Allowed when**: the command is given a folder path [book:cli] [op.rs].
- **Changes**: adds a `standard` project for that folder [book:cli] [person 2026-10-08]. The book also says this version has no such command; the CLI has it, so Add is in the contract [book:threads] [book:cli].
- **Refused when**: a path the command cannot use as a project folder — `invalid` [op.rs].

## Environment

An environment is the folder a thread works in, plus a status (`creating`, `provisioning`, `ready`, `error`, `destroyed`) [CONTEXT]. The providers are project, worktree, and personal [decision 2] [CONTEXT]. A project checkout is unmanaged and is never removed [CONTEXT] [decision 27]. A worktree is managed: kagan removes it a short grace after the last unarchived thread using it is archived, and it can still be reused while it is retiring [CONTEXT] [decision 26]. A personal environment is kept through archive and restart; only Delete removes it, after a person approves [decision 27].

### `Get`, `Branches`

- **Allowed when**: the id is known [book:cli]. `Branches` takes a project [book:cli]. `Get` takes an environment [book:cli].
- **Shows**: `Get` the environment, including the removal reason when git refused to delete the worktree [book:cli] [decision 28]. `Branches` the local branch names, and which one is current [book:cli]. `changes`, `pull-request` and `open` were removed as unused (decision 36).
- **Refused when**: a missing id — `not-found` [op.rs].

### `Restore`

- **Allowed when**: the thread had a managed environment whose folder was removed cleanly [book:threads] [decision 28].
- **Changes**: attaches the folder again from the branch that was left in place [book:threads] [decision 28]. A dirty worktree is never force-removed. A removal git refused because of files it would track stays `error`, names those files, and offers Remove again. Restore of that folder clears the reason and marks it `ready`. Restore stays available for a folder that was removed [decision 28] [decision 64] [docs:look]. A half-made managed environment that a restart finds with no thread is destroyed instead of restored [docs:architecture].
- **Shows**: the environment [book:cli].
- **Refused when**: a missing thread — `not-found` [op.rs]. A folder that cannot be attached — `failed` [op.rs].

### `Remove`

- **Allowed when**: a worktree whose removal git refused, so the environment is `error` and `Get` includes the reason [decision 28] [decision 64].
- **Changes**: runs teardown, then `git worktree remove` with no `--force` [decision 28]. Files git ignores go with the folder. Files git would track stay, and the reason names them [decision 64].
- **Shows**: done when the folder is gone [book:threads]. The refusal, when the folder is still there — `failed` [decision 28].
- **Refused when**: an environment that is not waiting on that refusal — `invalid` [op.rs].

### `SetBase`

- **Allowed when**: the environment exists [book:cli].
- **Changes**: the branch that change counts compare against [book:cli].
- **Shows**: the environment, with that base stored [book:cli]. The window does not name the base in a strip.
- **Refused when**: a missing environment — `not-found` [op.rs].

## Provider

The provider on a thread is the program line that thread runs, such as `claude` or `codex` [CONTEXT]. There is no registry and no install of agent programs.

## Plugin

A plugin is a package installed from git that adds UI, commands, tools, or skills [CONTEXT]. A plugin's thread record omits the program line [docs:plugin-api]. There is no trust shortcut for a plugin an agent built [docs:self-extend]. This contract has no catalog and no marketplace command; install is a git link or a local folder [docs:self-extend] [book:plugins]. Decision 3 describes a later index and is not an Op here [decision 3]. A manifest with `"areas": ["audio"]` is refused as an unknown area [decision 42].

### `List`, `Logs`, `Types`

- **Shows**: `List` the installed plugins [book:plugins]. `Logs` its log, crashes, and slow marks; `--follow` keeps printing [book:plugins] [docs:self-extend]. The log keeps the current file and one previous file, each at most 10 MiB; a cursor past the end returns `reset` and starts at 0; a record larger than 10 MiB is rejected [docs:self-extend]. `Types` rewrites the declaration files for this version [docs:self-extend].
- **Refused when**: a missing plugin id — `not-found` [op.rs]. A log record larger than 10 MiB — `invalid` [docs:self-extend] [op.rs]. Any other log or types failure — `failed` [op.rs].

### `Cli`

Not in `kg plugin` [book:cli]. It runs a command the plugin declares under `cli` [docs:plugin-api].

- **Shows**: exit code, stdout, and stderr [docs:plugin-api]. The project is the thread's project, else the project whose folder contains the working directory [docs:plugin-api].
- **Refused when**: the job runs past 120 seconds — `failed` [docs:plugin-api] [op.rs]. With no plugin host, the operation is unavailable — `failed` [docs:architecture] [op.rs]. That is the contract before plugins load [person 2026-10-08].

### `Command`

- **Allowed when**: the plugin declares the command, and its `when` matches: `always`, a thread on screen, or a project [docs:plugin-api] [book:cli].
- **Changes**: runs that command [book:cli]. The job has no reply body [docs:plugin-api].
- **Refused when**: past 10 seconds — `failed` [docs:plugin-api] [op.rs]. Unavailable without a plugin host — `failed` [docs:architecture] [op.rs]. That is the contract before plugins load [person 2026-10-08].

### `New`

- **Changes**: scaffolds a plugin folder: manifest, entry, and declaration files [docs:self-extend]. `--dir` chooses the folder [book:cli].
- **Refused when**: a bad name — `invalid` [op.rs].

### `Check`

- **Changes**: nothing. It checks the manifest (unknown keys, schemas, declared views exported) and does not run the plugin [docs:self-extend].
- **Refused when**: an invalid manifest — `invalid` [op.rs]. An unknown `powers.areas` entry, including `audio` and `downloads` — `invalid` [decision 42] [decision 65] [op.rs]. A `downloads` key or `powers.spawn` is refused [decision 65].

### `Reload`

- **Allowed when**: the plugin is installed [book:cli]. A person approves when powers grew [decision 14] [docs:self-extend]. A `downloads` key or `powers.spawn` is refused [decision 65].
- **Changes**: reloads the installed code [docs:self-extend]. Unload cancels the plugin's tasks, closes its job queue, fails its open jobs, kills processes it started, and releases its port [docs:plugin-api].
- **Refused when**: the person declines — `failed` [docs:self-extend] [book:plugins] [op.rs].

### `Dev`

- **Allowed when**: the path is an installed local plugin [docs:self-extend].
- **Changes**: watches the folder and reloads on each change; there is no build step [docs:self-extend]. Each reload uses the same approval as `Reload` [docs:architecture].
- **Refused when**: a declined approval — `failed` [docs:architecture] [book:plugins] [op.rs].

### `Remove`

- **Allowed when**: a person approves [decision 12] [book:plugins].
- **Changes**: removes the installed plugin [book:plugins]. Nothing is removed until they approve; declining ends the command [docs:plugin-api].
- **Refused when**: the person declines — `failed` [book:plugins] [op.rs]. An agent cannot answer [decision 10].

### `Install`

- **Allowed when**: the source is `git:<url>@<ref>` or a local folder [book:plugins]. A person approves the powers [decision 10] [book:plugins].
- **Changes**: installs that commit [decision 14]. A local folder is linked, not copied [book:plugins] [docs:self-extend]. `--plugin` names which plugin in a repository [book:cli]. The install screen shows powers and folder settings in plain words [docs:plugin-api]. A `downloads` key or `powers.spawn` is refused [decision 65]. `powers.read` entries must be folders under the home directory; the data directory, `~/.ssh`, and `~/.gnupg` are refused, and so is a path that escapes home [docs:plugin-api] [decision 44]. An older app than the manifest's `kagan` version refuses [docs:plugin-api].
- **Shows**: the powers question, then the installed plugin [book:plugins].
- **Refused when**: the person declines — `failed` [book:plugins] [op.rs]. A manifest the check would reject — `invalid` [docs:plugin-api] [op.rs].

### `Update`

- **Allowed when**: the plugin was installed from git [book:plugins] [docs:plugin-api].
- **Changes**: moves to the latest commit [book:cli]. The same code keeps running until then [book:plugins]. An update whose powers grew asks again [decision 14]. A `downloads` key or `powers.spawn` is refused [decision 65].
- **Refused when**: the person declines a wider update — `failed` [decision 14] [book:plugins] [op.rs].

### `Enable`

- **Allowed when**: the plugin is installed and disabled [book:plugins].
- **Changes**: loads it on next use [docs:plugin-api] [book:plugins]. Discover registers contributions without running the script; load runs on first use [docs:plugin-api].
- **Refused when**: a missing id — `not-found` [op.rs].

### `Disable`

- **Changes**: the plugin is not loaded [book:plugins]. A plugin whose renders stay over 16 ms is marked slow and the row offers disable; it is not unloaded on its own [decision 25].
- **Refused when**: a missing id — `not-found` [op.rs].

### `Config`

- **Changes**: reads, writes, or clears one plugin setting, through `Config.Get`, `Config.Set` and `Config.Clear` below [docs:plugin-api] [book:cli]. A folder setting is approved on the calling thread [book:cli].
- **Refused when**: a declined folder approval — `failed` [book:plugins] [op.rs].

### `Use`

- **Changes**: saves which plugin fills a slot, `threads` or `review`, as the preference `slot.<slot>`; `kagan` is the built-in. The window switches at once (decision 105) [op.rs].
- **Refused when**: the plugin is not enabled or offers no panel that replaces that slot — `invalid` [op.rs].

## Skill

A skill is a folder with a `SKILL.md`: a lowercase hyphenated `name` that matches the folder, a `description`, then the instructions [docs:self-extend] [book:skills]. Sources, lowest first: bundled, plugin, user (`<dataDir>/skills`), project (`<project>/.kagan/skills`) [docs:self-extend]. A higher source overrides a lower one with the same name [decision 24]. Two skills with the same name in one source are both dropped, and `List` shows the error [decision 24].

### `List`

- **Shows**: the visible skills and the per-source duplicate errors [decision 24] [book:skills]. `--project-id` picks the project skills [book:cli].
- **Refused when**: nothing in the sources names a hard failure. The duplicate case is reported in the result, not as a refusal [decision 24].

### `Show`

- **Allowed when**: one skill has that name after overrides [book:skills].
- **Shows**: the skill and its body [book:cli] [docs:self-extend].
- **Refused when**: an unknown name, including one dropped as a duplicate in its source — `not-found` [op.rs] [person 2026-10-08].

### `Files`

- **Shows**: the skill's file paths; bundled skills have none [docs:self-extend] [book:skills].
- **Refused when**: an unknown name — `not-found` [op.rs].

### `New`

- **Changes**: writes a skill folder into user skills, or into the project when `--project` is set [book:skills] [docs:self-extend].
- **Refused when**: a name that is not lowercase and hyphenated, or a name that already exists in that source — `invalid` [docs:self-extend] [op.rs] [person 2026-10-08].

### `Add`

- **Changes**: copies an existing skill folder into user skills, or into the project when `--project` is set [book:skills].
- **Refused when**: a path that is not a skill folder — `invalid` [op.rs].

### `Remove`

- **Allowed when**: the skill is a user or project skill [book:skills] [docs:self-extend].
- **Changes**: deletes that skill folder [book:skills].
- **Refused when**: a bundled or plugin skill — `invalid` [book:skills] [op.rs] [person 2026-10-08].

## Section

A section is a user-named group that threads can be filed under in the sidebar [CONTEXT].

### `Create`

- **Changes**: creates a section, and files the given thread under it when a thread id is passed [book:cli].
- **Refused when**: an empty name, or a name that already exists — `invalid` [person 2026-10-08] [op.rs].

### `Rename`

- **Changes**: renames the section [book:cli].
- **Refused when**: a missing section — `not-found` [op.rs]. An empty name — `invalid` [person 2026-10-08] [op.rs].

### `Delete`

- **Changes**: removes the section. Threads in it stay, unfiled [book:cli].
- **Refused when**: a missing section — `not-found` [op.rs].

### `File`

- **Allowed when**: the thread exists, including an archived thread [book:cli] [person 2026-10-08].
- **Changes**: files the thread under the section, or clears the section when the id is omitted [book:cli].
- **Refused when**: a missing thread or section — `not-found` [op.rs].

## Diff

A thread environment's diff reads git and never writes the worktree [book:cli]. Scopes are uncommitted, the branch against a base, and one commit [docs:look] [book:cli].

### `List`

- **Allowed when**: the thread has an environment git can read [book:cli]. `branch` may name a base; `commit` may name a rev [book:cli].
- **Shows**: the changed files in that scope [book:cli]. Each row shows the kind, the path, and the counts. Nothing marks a file as the agent's (decision 60).
- **Refused when**: a missing git or a missing base — `failed` [docs:architecture] [op.rs].

### `Show`

- **Allowed when**: the path is in that scope [book:cli].
- **Shows**: that file's diff and its new side [book:cli]. Opening it shows the review page in the center: the hunks at once, read-only (decision 60).
- **Refused when**: the same git errors as `List` — `failed` [docs:architecture] [op.rs].

### `Commits`

- **Shows**: commits that can be reviewed, newest first [book:cli].
- **Refused when**: a missing git — `failed` [docs:architecture] [op.rs].

## Ui

`kg ui` drives the native window without the operating system's mouse or keyboard, including while the window is in the background [docs:ui-driver]. Input goes through the window's own event dispatch [docs:ui-driver] [docs:architecture]. `--window` is a zero-based index into open windows; the default is 0 [docs:ui-driver]. The app must already be running with a window; `Open` installs the driver [docs:ui-driver]. Synthetic input cannot answer a person-only approval [decision 10] [docs:ui-driver].

### `Snapshot`

- **Allowed when**: a window is open [docs:ui-driver].
- **Shows**: the observed elements: stable ids, role, label, value, bounds, focused, disabled, and visible [docs:ui-driver] [book:cli].
- **Refused when**: no driver — "the ui driver needs a kagan window" [docs:ui-driver] — `failed` [op.rs].

### `Click`

- **Changes**: a left click at the element center [docs:ui-driver].
- **Refused when**: a missing element — `not-found` [op.rs]. A click that would answer a person-only approval returns "person-only approvals cannot be answered with kg ui" [docs:ui-driver] — `failed` [op.rs].

### `Type`

- **Changes**: sends the text to the focused control [docs:ui-driver].
- **Refused when**: the person-only message above, when the keys would answer [docs:ui-driver] — `failed` [op.rs].

### `Key`

- **Changes**: sends a keystroke such as escape or a shortcut [docs:ui-driver] [book:cli].
- **Refused when**: the person-only message above [docs:ui-driver] — `failed` [op.rs].

### `Scroll`

- **Changes**: scrolls the element by `dy` pixels [docs:ui-driver].
- **Refused when**: a missing element — `not-found` [op.rs].

### `Screenshot`

- **Shows**: a PNG at the given path, without focusing the window [docs:ui-driver] [book:cli]. A covered window captures the last presented buffer [docs:ui-driver].
- **Refused when**: a platform without capture — `failed` [docs:ui-driver] [op.rs]. No window — `failed` [docs:ui-driver] [op.rs].

## Theme

The app starts on `fjord`, in system mode, so it follows the operating system's light or dark mode [book:themes]. Modes are `light`, `dark` and `system` [book:themes]. A plugin does not add a theme; the manifest key is accepted and ignored [docs:self-extend]. Core stores the id and the mode; the window owns the files and the colors [docs:architecture].

### `Get`

- **Shows**: the selected theme id and mode [book:themes] [docs:look].

### `Set`

- **Allowed when**: the id is lowercase letters, digits, and dashes, and the mode is `light`, `dark` or `system` [book:themes] [docs:look]. `fjord`, `ember` and `grey` ship with the app [book:themes].
- **Changes**: selects that theme and mode for every open window [docs:look] [book:cli]. A custom theme is `<dataDir>/themes/<id>.json` [book:themes]. A well-formed id with no file yet stays selected; the window shows an error and uses the default palette for the mode [book:themes] [docs:look]. Saving a file updates the list; a missing or invalid selected file does the same and keeps the id [book:themes] [docs:look].
- **Refused when**: an id with any other character, a path, a dot, or traversal — `invalid` [docs:look] [op.rs].

## Keymap

Built-in shortcut overrides, and the keys of plugin commands, named `<plugin-id>:<command>` [book:cli] [decision 102]. On macOS the shortcut key is Command. On Windows and Linux a `secondary` chord is Control and Shift together, so a bare Control chord stays with the program [book:window]. One data directory is one machine, so a saved chord is not stored per platform. `kg keymap set --platform` was removed as unused (decision 54).

### `Get`

- **Shows**: the saved overrides under `shared`, and under `plugins` each enabled plugin command with its key, or null when it has none [decision 102]. Defaults, before an override, are NewThread `secondary-n`, StopThread `secondary-.`, ArchiveThread `secondary-shift-backspace`, TogglePinned `secondary-shift-p`, OpenSettings `secondary-,`, UseProjectFolder `secondary-alt-p`, UseWorktreeFolder `secondary-alt-w`, RetryBranches `secondary-alt-r`, RetryPrograms `secondary-alt-l`, ToggleCustomCommand `secondary-alt-c`, ThreadActions `secondary-shift-m`, ToggleSidebar `secondary-b`, ToggleRightSidebar `secondary-\`, AddProject `secondary-shift-o`, NextHunk `alt-]`, PrevHunk `alt-[`, FindInDiff `secondary-f`, NextMatch `secondary-g`, PrevMatch `secondary-alt-g`, StartAgain `secondary-shift-s`, FocusSidebar `secondary-shift-l`, RestoreThread `secondary-shift-r`, UseBuiltInSidebar `secondary-alt-b`, UseBuiltInReview `secondary-alt-v`, OpenCommandList `secondary-shift-a`, and SearchThreads `secondary-alt-f` [op.rs]. Off macOS a `secondary` chord is bound as `ctrl-shift` plus the rest, and a following `shift-` is already in that. A saved override is used as saved. ShowHelp, NextFile and PrevFile were removed as unused (decision 54). The command list came back as OpenCommandList (decision 107).

### `Set`

- **Allowed when**: the action is one of those defaults, or a command of an enabled plugin [op.rs] [book:cli] [decision 102].
- **Changes**: records the chord for this machine [book:cli]. `--reset` drops the saved chord and uses the default key [book:cli]. A conflicting chord is not stored; the result names the other action [op.rs]. A plugin command's key is refused when it overlaps a window key or another plugin command's key [docs:plugin-api]. It is stored with that plugin and applies while the app is in front [decision 102].
- **Refused when**: an unknown action, or a plugin that is not enabled or has no such command — `invalid` or `not-found` [op.rs]. An empty key or an unknown key name — `invalid` [op.rs].

### `Remove`

- **Changes**: leaves a plugin command with no key, until it is set or reset [decision 102].
- **Refused when**: a built-in shortcut, which always keeps a key — `invalid` [decision 102].

## Preference

Exactly these keys exist: `confirm-archive` [person 2026-10-08], `agent-integration`, `notify` [settings.rs], and the sidebar layout of each window, `<window>.left-sidebar`, `<window>.left-width`, `<window>.right-sidebar` and `<window>.right-width`, where `<window>` is a lowercase id such as `main` [decision 48]. `row-actions` and `footer` were removed as unused: the window never read them. The bare `right-sidebar` name was removed as unused (decision 48). The dock layout is not a preference and not an `Op`; it is `{dataDir}/layout.json` [decision 15]. `predict` still loads and no longer hides hunks; it is not one of these keys [decision 43].

### `Get`

- **Shows**: the stored value. An unset key returns null [person 2026-10-08]. The window treats an unset `confirm-archive` as off [person 2026-10-08].
- **Refused when**: any other key — `invalid`, message `unknown preference <key>` [person 2026-10-08] [op.rs].

### `Set`

- **Changes**: stores the value when the shape matches [person 2026-10-08]. `confirm-archive` is a boolean [person 2026-10-08], and so are `agent-integration` and `notify` [settings.rs]. `notify` set to `false` stops the system notification for a thread that needs you while that thread is not on screen. An unset `notify` counts as on [notify.rs]. `<window>.right-sidebar` is a boolean that says whether the right sidebar is open; an open window reads it when it opens, so a change applies the next time the window opens [decision 48].
- **Refused when**: any other key — `invalid`, message `unknown preference <key>` [person 2026-10-08] [op.rs]. A wrong shape — `invalid` [person 2026-10-08] [op.rs].

## Config

Plugin settings, reached as `kg plugin config <id> get|set|clear` [docs:plugin-api] [book:cli]. Setting types are string, boolean, select, folder, secret, and provider [docs:plugin-api]. A provider setting is left out of the settings read. A thread a plugin starts names its program in `spawn` [docs:plugin-api].

### `Get`

- **Shows**: the setting value [book:cli]. A provider setting is left out of the plugin's settings read [docs:plugin-api]. A secret setting shows `"set"` or `null`, never its text, to every caller [decision 104].
- **Refused when**: a key the manifest does not declare — `invalid` [docs:plugin-api] [op.rs].

### `Set`

- **Allowed when**: the key is one the manifest declares [docs:plugin-api]. A folder value is approved by a person on the calling thread [book:cli] [docs:plugin-api].
- **Changes**: stores the value [book:cli]. A folder setting is granted read-only [docs:plugin-api]. `kg` reads a value left off the command line from a prompt that does not show it, or from stdin, as text [book:cli]. A secret goes to the system keychain, or to a file only the person can read with a warning, never the database [decision 104].
- **Refused when**: the person declines a folder — `failed` [book:plugins] [op.rs]. A value of the wrong type — `invalid` [op.rs]. No value — `invalid` [op.rs]. A secret that is empty, more than one line, or over 1024 bytes — `invalid`, and the message does not repeat it [decision 104].

### `Clear`

- **Allowed when**: the key is a secret setting the manifest declares [decision 104].
- **Changes**: forgets the secret; the plugin's settings read shows `null` [decision 104].
- **Refused when**: any other setting — `invalid` [op.rs].

## Window

The macOS menu bar. These shortcuts are not keymap overrides. Quit asks when a program is running [decision 66].

### `About`

- **Allowed when**: always. The kagan mark's menu on the rail runs it, and the same menu runs `SettingsOp::Open` for Settings… [decision 108].
- **Shows**: its own window with the app icon, kagan, the version with the short commit of the build, a tagline, the GitHub, Book, License and Acknowledgements links, and the copyright. A second About focuses the open window. Escape, Cmd+W, or a click in another window closes it [decision 99] [docs:look].

### `Hide`, `HideOthers`, `ShowAll`

- **Changes**: hides kagan, hides the other applications, or shows them [docs:look].

### `Quit`

- **Changes**: quits at once when nothing is running. When a program is running, the menu, Cmd+Q, and the Dock ask once. Quit ends every running program. Cancel leaves them. There is no setting to skip the question. `kg window quit` does not ask [decision 66]. Programs still running end. The next open marks a thread left active failed with "kagan quit while the program was running" and says no output was saved [decision 46] [decision 63].
- **Shows**: the app exits [docs:look].

### `Minimize`, `Zoom`

- **Changes**: minimizes or zooms the window [docs:look].

### `Close`

- **Changes**: closes the window. Programs keep running [decision 46] [docs:look]. Clicking the Dock icon opens the window again [decision 66].

### `CommandList`, `SearchThreads`

- **Changes**: opens the window when it is closed, and shows the command list, or the list on thread search. When the list is open, each switches it to its own view [decision 107].
- **Shows**: every window command and each enabled plugin's commands, grouped, with the key that runs it from where the keyboard was. A command that cannot run there is greyed with its reason. Enter runs it from there; Escape or a click outside closes the list and gives the keyboard back. Thread search shows the same rows as `kg thread search --archived`, under Threads and Archived; Enter opens the thread at the matched line [decision 107].

## Status, Guide, Open, Sidebar

### `Status`

- **Shows**: the current thread, project, and environment when passed or taken from the environment, plus the `kg` path, the data directory, and warnings such as a plugin that failed to load [book:cli] [docs:self-extend].
- **Refused when**: a thread id that is empty or contains a character other than an ASCII letter, digit, `_`, or `-` — `invalid` [op.rs]. A missing thread — `not-found` [op.rs].

### `Guide`

- **Shows**: the concepts, and one chapter when named [book:cli] [op.rs]. The text ships in the binary [docs:self-extend]. Chapters are threads, projects, environments, plugins, skills, ui, and json [op.rs] [book:cli].
- **Refused when**: an unknown chapter — `invalid` [op.rs].

### `Open`

- **Changes**: asks the app to open a native window in the already running process [book:cli] [docs:architecture]. With no process, `kg` starts one [docs:architecture]. The app subscribes for this before plugins start, and keeps that subscription with no window open [docs:architecture].
- **Shows**: a window [book:start].

### `Sidebar`

- **Shows**: projects, sections, and the threads the sidebar shows [book:cli] [docs:plugin-api]. It omits archived threads [decision 41] [docs:look] and hidden threads [CONTEXT].
- **Refused when**: nothing in the sources.

## Cross-Op rules

- Only a person answers an install, a thread delete, or a standalone approval. Answering is not an `Op`. `kg ui` cannot answer, and synthetic `kg ui` cannot answer [decision 10]. A plugin may answer another pending question on a thread through `Workspace::answer`. That removes the pending row and writes `answered`. Deleting the row is not an answer [docs:plugin-api]. Every answer appends `answered` with the interaction's title and the chosen option. That is the trace of an approval on the thread [docs:architecture] [event.rs].
- Archive, unarchive, and stop never ask [decision 12] [docs:architecture] [docs:plugin-api].
- The database is the truth for thread rows and the event log [decision 5] [docs:architecture].
- Opening the app does not resume a program. A thread left active is marked failed with "kagan quit while the program was running". A thread left starting is marked failed with "kagan stopped while the thread was starting". A thread left stopping is marked failed. Archive and environment-removal deadlines resume [docs:architecture].
- A live thread does not append message events [docs:architecture].
- A restart drops standalone approvals, because their responders are gone [decision 38] [docs:architecture]. A question with no window open waits until a person answers; it does not time out [decision 13].
- Stopping a thread that is still starting leaves it idle. Stopping a running thread marks it `stopping`; once the program has ended it records `stopped` and `idle` and goes idle, with no attention. A program that exits 0 records `exited` and `idle`. Any other exit records `exited` and `failed`, leaves the thread in `error`, and writes no `idle`. A terminal program has no turn to end [docs:architecture].
- A child left active or starting appends a notice on its sidebar parent. A thread left stopping stays quiet. The lifecycle owner gets no copy [decision 19].
- Parent is sidebar nesting only. The lifecycle owner is set at creation and is not changed later. Archiving or deleting the owner does the same to its dependents [decision 20] [CONTEXT].
- Attention moves when a thread goes to error or raises a question. Going from busy to waiting or exiting successfully sets done instead [decision 94]. Opening the thread marks it read. Unread is not raised on every agent message [decision 22]. Other threads' status shows in the sidebar as it changes. The title bar does not count them [decision 43].
- No file is marked as the agent's. `file.changed` and `file.read` have no producer (decision 60).
- One 10 second grace covers archive and managed-environment removal. Both deadlines persist across a restart [decision 26].
- Removal deletes only folders kagan created. A worktree is removed only after the grace. A personal folder is removed only by Delete, after a person approves a question naming the folder, and never while another thread, project or environment is in or on it. A project checkout is never removed [decision 27].
- A dirty worktree is never force-removed. The exception is a folder that `git worktree add` itself failed to create, which never became a ready environment [decision 28]. A refused removal names the files git would keep. `kg environment remove` tries again after they are committed, deleted, or git-ignored [decision 64].
- A hidden thread stays reachable by id. Delete still asks [decision 21].
- Two skills with the same name in one source are both dropped. A higher source overrides a lower one: bundled, plugin, user, project [decision 24].
- A plugin's thread record omits the program line. A thread it starts names the program in `spawn` [docs:plugin-api].
- Built-in thread commands include `stop` and `archive` [docs:architecture].
- Every `kg` command takes `--json`. A failure prints `{ "error": { "kind", "message" } }` and exits non-zero. The kinds are `not-found`, `invalid`, and `failed` [docs:self-extend] [book:cli].
- Dropping the caller's task does not cancel an operation that was accepted [docs:architecture].
- An attachment embeds up to 8 MiB. A larger file stays a link [decision 16].
- A phone client is specified and is not part of the process that ships. The plugin loopback port is removed [decision 37].

## Removed

Step 1 deleted these on purpose. The model suite does not assert them.

- The product is not archived: `ARCHIVED.md` and the README archive banner are gone (#152).
- No voice or audio: no dictation, no spoken replies, no voice command, no `kg-audio` crate, no `speech` feature, and a plugin manifest with an `audio` area is an unknown area (#153) [decision 42].
- No prediction: opening an unread file's diff shows the hunks at once and clears the mark (#154) [decision 43].
- No answer line: a permission question shows only its options; an answer stores the choice and the title, with no line (#154) [decision 43].
- No held news: when another thread finishes, the sidebar shows its new status at once, and the title bar has no waiting count (#154) [decision 43].
- No attention report: `kg usage attention` is not a command (#154) [decision 43].
- No unread-diffs preference. The book phrase is gone with that report (#154) [decision 43].
- No Usage page, no cost roll-up, and no token line (#155) [decision 44].
- `opened` and `file.predicted` are no longer written [decision 43].
- No in-app browser. `kg browser` is not a command, and `openBrowser` is not a host function. The window never showed a browser view, every driving command failed before it reached a page, and no plugin opened one. `openUrl` still opens the system browser [decision 31].
