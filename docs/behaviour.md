# Behaviour

What each `Op` does, one rule per line, for a reader who has the `Op` signatures and not the implementation. If this file and a test disagree, the test is right and this file has a bug.

Sources, in order: [CONTEXT], [decision N], [book:page], [docs:file], [op/] (the `crates/domain/src/op/` modules), [event.rs], [record.rs]. Help text is the CLI contract in [book:cli].

A missing id is `not-found`. A bad argument is `invalid`. A pinned setup script that changed before Start is `setup-changed` [decision 192]. Every other refusal is `failed` [op/]. An `Op` request uses the kebab-case name and camelCase fields, like the `OpResult` reply.

## Questions for the person

All questions answered by the person on 2026-10-08.

## Thread

A thread is one program running in one environment, with a status (`pending`, `idle`, `starting`, `active`, `stopping`, `error`) and an append-only event log [CONTEXT]. The program line is stored on the thread. Unread means its attention is newer than its last read [CONTEXT].

### `Get`, `Metadata`, `Events`, `Interactions`

- **Allowed when**: the thread id is known [book:cli]. These reads are allowed in every status, including an archived thread [person 2026-10-08].
- **Shows**: `Get` returns the thread. The stored provider is the program line [record.rs] [op/]. A plugin's own thread record omits the program [docs:plugin-api]. `Metadata` that plugin's value on the thread [book:cli]. `Events` the log after `after_seq`, limited, in `asc` or `desc` order [book:cli]. The types are `created`, `started`, `exited`, `stopped`, `idle`, `waiting`, `failed`, `archived`, `unarchived`, `deleted`, `turn.started`, `turn.ended`, `message`, `tool`, `tool.started`, `tool.ended`, `restore.failed`, `snapshot.failed`, `notice`, `answered`, `file.changed`, `file.read`, `file.predicted`, `opened`, `rewound`, and `reconnecting` [event.rs]. `turn.started` and `turn.ended` are written from the turn's source, the hook, the spinner, or quiet output [decision 235]. Types kagan no longer writes stay in the plugin event list. A live thread appends a user message when `Send` submits a line [decision 136]. It does not append a message from the screen [docs:architecture]. `Interactions` the pending questions; each shows its options [decision 43] [book:cli].
- **Refused when**: the thread is missing — `not-found` [op/].

### `List`

- **Allowed when**: always [book:cli].
- **Shows**: threads for one project when a project is set, limited, and before a timestamp [book:cli]. `--archived true` lists archived threads; `--archived false` lists threads that are not [book:cli]. An omitted `--archived` returns both [person 2026-10-08]. Hidden threads are included [person 2026-10-08].

### `Run`

- **Allowed when**: the project exists and the program resolves on the login `PATH`, or in the folder its installer puts it in by default under the home folder (`opencode` in `~/.opencode/bin`, `cursor-agent` and `goose` in `~/.local/bin`, `amp` in `~/.amp/bin`; `PROGRAM_INSTALL_FOLDERS`) [op/] [decision 237].
- **Changes**: starts a thread in the chosen environment [op/]. The environment defaults to `worktree` [op/]. `project-default` leaves the project folder on its current branch [decision 170]. The status is `starting` [op/]. The stored provider is the full program line [op/]. Parent, quiet, lifecycle owner, visibility, and metadata are set here [docs:plugin-api]. `--quiet` keeps the parent from being typed a line about this thread [decision 201]. The host starts that program in the thread's folder, in that thread's terminal, and keeps it running whether or not a window is open [decision 46]. When agent integration is on, the host adds the note and the program flags for the programs decision 172 names. Kimi, OpenCode, and a program kagan does not recognize stay at the `KG_*` variables and `PATH` [decision 172]. Opening the app starts that program again when the thread was still running [decision 175]. `project-on:checkout:<branch>` checks out that local branch in the project folder before the thread starts, and `project-on:new:<base>` creates a branch there from that local branch; the new name is the thread title, the same way a worktree branch is named [decision 170]. A worktree's base may be a chosen local branch, and the folder is created from that branch's commit [decision 64] [decision 170]. Checking out the branch already current does not move HEAD.
- **Shows**: the new thread [book:cli].
- **Refused when**: an empty program — `program is empty` — `invalid` [op/]. An unfinished quote — `program has an unfinished quote` — `invalid` [op/]. A missing program — `program <name> was not found` — `invalid` [op/]. A program that cannot start — `failed` with the reason; the thread is left in `error` with a `failed` event [decision 46]. A worktree on Personal — `git worktrees need a standard project` — `invalid` [op/]. A branch change on anything but a standard project — `Changing branches needs a standard project.` — `invalid` [decision 170]. A create or switch while the checkout has uncommitted changes — `Commit or stash the uncommitted changes in this checkout to create or switch branches.` — `invalid`. While HEAD is detached — `HEAD is detached. Check out a branch here before creating or switching branches.` — `invalid`. With no commit yet — `Create the first commit in this checkout to create or switch branches.` — `invalid`. During a rebase — `Finish or abort the rebase before changing branches.` — `invalid`. During a cherry-pick — `Finish or abort the cherry-pick before changing branches.` — `invalid`. During a revert — `Finish or abort the revert before changing branches.` — `invalid`. During a merge with conflicts — `Resolve the conflicts and finish or abort the merge before changing branches.` — `invalid`. During a merge otherwise — `Finish or abort the merge before changing branches.` — `invalid`. A branch that is not local, including the base of a new branch — `branch <name> is not in this repository` — `invalid`. A new name that already exists — `branch <name> already exists` — `invalid`. A name git would refuse — `branch name <name> is not allowed` — `invalid`. Another unarchived thread in that folder — `Another thread is using this folder.` — `invalid`. A blocked checkout is reported before another thread in that folder.

### `Stop`

- **Allowed when**: the thread exists, including one that is archived [book:cli]. On an archived thread it ends the run now instead of waiting for the archive grace [decision 138].
- **Changes**: a thread that is still starting becomes idle. A running thread is marked `stopping`; once its program has ended, it records `stopped` and `idle` and becomes idle, with no attention. A turn ends when the program's own hook says so, when its spinner goes plain, when a known agent with no hook shows no change in its visible text for 15 seconds, when a hook-run turn is silent for 120 seconds, or when the program ends [decision 235]. Stopping during an open turn records `turn.ended` with `interrupted` beside `stopped` and `idle` [decision 123]. The program and what it started end at once, except a process that calls `setsid` itself; on Windows the program's Job Object ends what it started, except a process started before the program joined the job [decision 46] [decision 110]. It does not ask [decision 12].
- **Shows**: the thread leaves the running status [book:window].
- **Refused when**: a missing thread — `not-found` [op/].

### `StartAgain`

- **Allowed when**: the thread exists, is not archived, its program is not running, and its folder is ready [decision 54].
- **Changes**: runs the thread's program again in the same thread and folder, at the size the window last gave it [decision 54]. A `claude` thread resumes its conversation with `--resume` and the id kagan gave it at the first start with `--session-id`. When the thread has no id, or Claude Code has no conversation with it, the agent starts a new conversation with a new id and the thread gets an info notice. A `codex` line with no arguments resumes with `codex resume <id>`: the thread's saved id, or else the one Codex conversation that began in the thread's folder since its last start, which kagan then saves. None, two, or a line with other arguments start a new conversation with an info notice. A `cursor-agent` or `agent` thread gets an id at its first start with `--new-session-id`. Start again passes `--resume` and that id when that conversation's directory is still under `chats` in `CURSOR_CONFIG_DIR`, `$XDG_CONFIG_HOME/cursor`, or `~/.cursor`, and the folder recorded there is this thread's, or none is recorded yet. No id, a missing directory, or a recorded folder that is a different one starts a new conversation with a new id and the same info notice. `--continue` is not added. An `opencode` thread gets an id at its first start with `--session`, which creates the session when it is new, and Start again passes that same id. A `pi` thread gets `--session-id` the same way. A `kimi` line with no arguments resumes with `--session session_<id>` when Kimi Code has one session for this folder since the last start under `KIMI_CODE_HOME` or `~/.kimi-code`. For `codex` and `kimi`, that one file is saved on the thread while the program is running. A later file does not replace a saved id whose file is still there. An id another thread already holds is left alone, and two files before one was saved start a new conversation. When the saved file is gone, the next file that run writes replaces the id. None or two, before one was saved, start a new conversation with an info notice. A line that already picks its conversation (for `codex`, one that names `resume` or `fork`; for `cursor-agent` and `agent`, one that passes `--resume`, `--continue`, or `--new-session-id`, or names a subcommand; for `opencode`, `--session`, `-s`, `--continue`, or `-c`; for `pi`, `--session`, `--session-id`, `--continue`, `-c`, `--resume`, `-r`, `--fork`, or `--no-session`; for `kimi`, `--session`, `-S`, `--continue`, or `-c`), and every line with agent integration off, gets nothing added and no notice [decision 54] [decision 176] [decision 49]. Opening the app does this for a thread that was still running: active or starting, or idle because quit stopped it [decision 175]. A thread the person stopped, one that exited, and an archived thread stay as they are. A thread left stopping because the person asked it to stop is marked failed and is not started again [decision 175].
- **Shows**: the thread, now `starting` or `active` [op/].
- **Refused when**: a running thread — `the program is still running` — `invalid`. An archived thread — `restore the thread before starting it again` — `invalid`. A folder that is not ready — `invalid` [op/].

- **New thread here… (the ended line, not an op)**: shown for an ended thread that is not archived, whose folder is still there, belongs to its project and is ready (the same test core applies when it runs there). It opens the new-thread screen with the thread's folder as the only choice, shown as one line with the folder's name, and its full path in a tooltip, under a title that names the thread. Start runs `ThreadOp::Run` with `environment: reuse:<environment id>` and the program the person picks [decision 199]. Cancel and Escape return to the ended thread.

### `Input`, `Paste`, `Send`, `Report`, `Resize`, `Scroll`, `Select`, `Copy`, `Screen`

- **Allowed when**: the thread's program is running in the host [decision 46].
- **Changes**: `Input` writes text to the program's terminal and brings the view back to the bottom. `Paste` does the same, wrapped in bracketed-paste markers when the program asked for them; otherwise its newlines become carriage returns. A paste that names a path is `invalid` and sends nothing; `kg` reads `--file`, or stdin when the path is `-`, up to 1 MiB, and sends the text. Escape and the other control characters are removed from pasted text, so it cannot end a bracketed paste or type a command. `Send` types text the same way, then presses Enter, as one write, up to 1 MiB. It can run while the program is busy and appends to input already on the line. Without bracketed paste, embedded newlines become carriage returns inside that write. Empty text and text that is only control characters are `invalid`. A program that has ended, and a write that fails, are refused and write no `message`. The typed text is a user `message` only after the write succeeds [decision 136]. It does not queue. `Report` sends mouse-report bytes (decimal values, 0 to 255) to the program without that scroll reset, so a scrolled view stays where it is. `Resize` sets the program's size, from 2 to 1000 columns and 1 to 500 rows. The last client to resize sets it [decision 46]. `Scroll` moves the view by lines into the history, up when positive. Each program keeps 10,000 lines of history. `Select` starts a selection at a cell (`simple`, `word`, `line` or `block`), or moves the end of the one there is (`extend`). `Screen` changes nothing.
- **Shows**: `Screen` returns the visible rows as an ANSI redraw, with the modes that change what keys send [decision 46]. With `--scrollback` it returns every kept line as plain text. `--lines` is refused without `--scrollback`; with it, the count is from 1 to 10000 and the result is that many last rows of the same text, or the whole text when there are fewer. A plugin's `threads.output` is that scrollback plain text, newest 256 KiB [decision 128]. `Copy` returns the selected text, or nothing when no text is selected; a whole line copied has no newline at its end. `Input`, `Paste`, `Send`, `Report`, `Resize`, `Scroll` and `Select` return done. `WorkspaceEvent::Screen` follows a change, at most about 30 times a second [decision 46].
- **Refused when**: the thread has no running program — `not-found` [op/]. A size outside those limits — `invalid` [op/]. `--lines` without `--scrollback`, or a count outside 1 to 10000 — `invalid` [op/]. A settled screen is kept across a restart, at most 4 MiB for the whole snapshot [decision 62]. A crash while the program is still running leaves the reason and no screen. The thread says no output was saved for that run. An ended program whose screen was kept still answers `Screen` and `threads.output`.

### `Terminal` (`New`, `List`, `Close`)

- **Allowed when**: the thread exists. `New` also needs it not archived and its folder ready [decision 110].
- **Changes**: `New` starts the login shell in the thread's folder with the thread's `KG_*` variables and gives it the next number; numbers are not reused while the thread lives. `Close` ends that shell and what it started: its process group on macOS and Linux, except a process that calls `setsid` itself, and its Job Object on Windows, except a process started before the shell joined the job. A shell that exits closes itself. Archive ends a thread's extra terminals when its 10-second grace ends, whether or not the program still runs; Undo within the grace keeps them. Delete and Quit end them at once. None is restored after a restart, and none writes an event or changes the thread's status or marks [decision 110].
- **Shows**: `New` returns the new terminal; `List` returns each open one with its number and the title its shell set. The terminal Ops above reach one with `--terminal <n>` [decision 110].
- **Refused when**: a missing thread, or a number that is not open — `not-found` [op/]. `New` on an archived thread or a folder that is not ready — `invalid`. A shell that cannot start — `failed` [decision 110].

### Terminal in the window

- The wheel and trackpad scroll the history. Shift+PageUp and Shift+PageDown page it. Output that arrives while the view is scrolled up does not move it; typing does.
- Drag selects, double-click selects a word, and triple-click selects a line. The selection is drawn in the theme's selection color. Releasing the mouse copies that text. `terminal-copy-on-select` is on when unset, and saved `false` leaves the clipboard alone. A selection that ends empty, such as a plain click, copies nothing [decision 231].
- Copy is Cmd+C, or Ctrl+Shift+C off macOS. Paste is Cmd+V, or Ctrl+Shift+V off macOS.
- A program that asks for mouse reports (DECSET 1000, 1002 or 1003) gets clicks, drags and the wheel. It gets them in the SGR form when it asks for 1006, and in the legacy form otherwise. Shift overrides this: the press selects and the wheel scrolls in the window. A release reported to the program copies nothing. Shift-drag is this window's selection and copies [decision 231].
- Terminal in the right sidebar's + opens an extra terminal as a tab of that sidebar, selects it and gives it the keys. Cmd+T does the same and opens the sidebar when it is hidden. The center shows only the program's terminal. Cmd+Alt+] and Cmd+Alt+[ step the keys through the program's terminal and each extra terminal. × on a tab, or Cmd+Shift+W in an extra terminal, closes it, and the keys go to the selected tab or the +. A terminal opened with `kg` adds a tab and does not change the selected one. Showing another thread shows its terminals [decisions 110 and 118].
- Browser in the right sidebar's + opens the Browser tab and gives its address field the keys. Only http and https open, from the field and for links inside the page, and a bare name such as `example.com` gets https, or http for a local host such as `localhost:3000`. The page appears on the first address and is hidden whenever its tab is not drawn. Closing the tab or showing another thread drops it and clears the address. Its cookies and logins end when the page closes, and never gets the camera or microphone. On Linux the page is WebKitGTK. When `DISPLAY` is set the window uses X11 so the page can sit in it; without an X11 display the page says so [decision 164]. The address row has Back, Forward, Reload, and Open in external browser. Back, Forward and Reload stay off until a page is open. Open in external browser opens that address in the system browser and stays off until the address is a web address. An https address shows a lock. While a thread drives the page, the address field stays hidden and those four controls stay on the row with Driven by and Stop agent. A plugin with the browser area drives this same page through `kagan/browser`, with the same verbs as `kg browser`, and it does not add a second Browser tab [decisions 117 and 161]. A thread's agent can drive it with `kg browser`: the tab then shows `Driven by <thread>` and Stop agent instead of the address field, and showing another thread hides the page without ending the browser session. Stop agent, or `kg browser close`, leaves the page as the person's with its address in the field [decision 143].
- Cmd-click, or Ctrl-click off macOS, on an http or https link opens it in the system browser and does nothing else: no selection and no mouse report. A link the program set with OSC 8 opens its target; other links are read from the text, across wrapped rows [decision 112].
- When the thread's terminals printed a local link (localhost, 127.0.0.1, 0.0.0.0, [::1] or [::]), the row over the terminal shows Open host:port at its right end for the newest one. The button, Cmd+Alt+O or `kg window open-local-link` opens it [decision 112].

### `Archive`

- **Allowed when**: the thread exists [book:cli]. Allowed in every status, including a thread that is already archived [person 2026-10-08].
- **Changes**: the history stays; the run stops after 10 seconds; unarchive in that window cancels the stop [book:threads] [decision 26]. The thread leaves the sidebar [decision 41] [CONTEXT]. Archiving a lifecycle owner does the same to its dependents [decision 20] [CONTEXT]. A managed environment starts a 10 second retirement when it has no unarchived thread left [decision 26] [book:threads]. The Op never asks [decision 12] [person 2026-10-08].
- **Shows**: the thread is listed on Archived [decision 41] [book:threads]. The window asks first only when `confirm-archive` is on [person 2026-10-08].
- **Refused when**: a missing thread — `not-found` [op/].

### `Unarchive`

- **Allowed when**: the thread exists [decision 138]. A live thread is already unarchived; the call returns done and writes no event.
- **Changes**: brings an archived thread back [book:threads]. Inside the 10 second grace, the stop does not happen [book:threads]. Reuse of its environment clears retirement [docs:architecture]. It does not ask [decision 12].
- **Shows**: the thread can return to the sidebar [decision 41].
- **Refused when**: a missing thread — `not-found` [op/]. The lifecycle owner is still archived — `invalid`, message `unarchive the lifecycle owner first`.

### `Delete`

- **Allowed when**: a person approves [decision 12] [docs:architecture].
- **Changes**: removes the thread and its history for good, and the threads that name it as lifecycle owner [CONTEXT] [decision 20]. Declining removes nothing [docs:architecture].
- **Shows**: a pending interaction on the thread named for the approval, or a standalone question when the command names no thread [docs:architecture] [book:cli]. The question shows its options [decision 43].
- **Refused when**: the person declines — `failed` [book:plugins] [op/]. An agent cannot answer it [decision 10]. A missing thread — `not-found` [op/]. A worktree whose removal was refused — `invalid`. The thread stays until Remove again [decision 28].

### `SetPinned`

- **Allowed when**: the thread exists [book:cli].
- **Changes**: pins it to the top of the sidebar, or unpins it [CONTEXT] [book:cli].
- **Refused when**: a missing thread — `not-found` [op/].

### `Notify`

- **Allowed when**: the thread exists [book:cli].
- **Changes**: sets the thread's own notification level: `default` follows the global `notify`, `all` and `asks` replace it with that level, and `muted` posts nothing for this thread. A muted thread shows a muted mark beside its title and in its sidebar row. It writes no event [decision 213].
- **Refused when**: a missing thread — `not-found` [op/].

### `Rename`

- **Allowed when**: the thread exists, including one that is archived [book:cli].
- **Changes**: a rename in the window stores the trimmed title [book:cli]. A rename from the socket stores it once; a later one leaves the locked title as it is [record.rs]. The program, the environment, and the branch stay. It writes no event.
- **Shows**: the thread, with that title, when the title changed [book:cli].
- **Refused when**: a blank title — `invalid` [op/]. A locked title renamed from the socket — `invalid`, and the title stays [record.rs]. A missing thread — `not-found` [op/].

### `MarkRead`

- **Allowed when**: the thread exists [book:cli]. Opening the thread marks it read [decision 22].
- **Changes**: the thread is no longer unread [CONTEXT] [decision 22]. Attention itself moves on idle, on error, and when a question is raised [decision 22]. It does not append `opened`; that type is no longer written [decision 43].
- **Refused when**: a missing thread — `not-found` [op/].

### `SetMetadata`

- **Allowed when**: the thread exists and the caller is setting that plugin's own value [docs:plugin-api] [book:cli].
- **Changes**: stores that plugin's metadata on the thread [book:cli]. A tool with scope `tagged` is offered only on threads that carry it [docs:plugin-api].
- **Refused when**: a missing thread — `not-found` [op/].

### `WaitEvent`

- **Allowed when**: the thread exists [book:cli].
- **Shows**: the next event of that kind after `after_seq`, or no event when `timeout_ms` passes [docs:plugin-api] [op/]. The kind is one of the live event types listed under `Events` [event.rs].
- **Refused when**: an unknown kind — `invalid` [op/]. A missing thread — `not-found` [op/]. A timeout is not a refusal [op/].

### `Wait`

- **Allowed when**: the thread exists [book:cli].
- **Shows**: `waiting` after the turn ends, `exited` when the row is idle (a clean exit or Stop), `failed` when the program failed, `not-busy` after two seconds with no open turn, or `timeout` when `timeout_ms` passes [op/]. `not-busy` waits out that arm. A timeout leaves the thread alone.
- **Refused when**: a missing thread — `not-found` [op/].

### `Turn`

- **Allowed when**: the thread exists. A `start` for a program that is not running is ignored [decision 235].
- **Changes**: `start` opens a turn and records `turn.started`; `end` closes it and records `turn.ended` and `waiting`. A `start` while a turn is open, or an `end` while none is, changes nothing, so a late `end` is harmless [decision 235].
- **Shows**: nothing. An agent's own hook calls it, so it is hidden from `kg --help`. It sends to a running app and otherwise does nothing: it never starts an app, and it always exits 0 [decision 235].
- **Refused when**: a missing thread — `not-found` [op/].

### `Ask`

- **Allowed when**: the thread exists and is not archived [op/].
- **Changes**: stores a question with that title, those options, and its fields when it declares any, for the person to answer. A field is a `string`, `boolean`, `number`, or `select`; a number answer is a whole number (`i64`). A select carries choices; an optional one also has a None item that answers `null`, and `required: true` drops that item [decision 144]. A second ask while that question is open does not add another [op/].
- **Shows**: the interaction id at once. The person's choice is an `answered` event carrying the title, the option id, and one value per field (empty when no fields were declared) [event.rs]. Questions draws that title and those options from the pending row.
- **Refused when**: an empty title, fewer than two options, a field or choice with an empty id or label, a repeated field id, a repeated choice id, a select with no choice, or a select default that is not one of its choices — `invalid` [op/]. Also `invalid`: more than 32 fields, more than 32 choices in one select, an id over 64 characters, a label over 128, or a placeholder, default, or string answer over 4096 [op/]. Nothing is stored. A missing thread — `not-found` [op/]. An archived thread — `invalid` [op/]. An answer that misses a declared field, names a field the question did not declare, gives a value of the wrong kind, or answers `null` to a required select — `invalid`, and the question stays [op/]. A plugin cannot answer it, and the card stays for the person [docs:plugin-api].


### `Tools`

- **Allowed when**: the thread exists [book:cli].
- **Shows**: the tools offered to that thread, and the plugin instructions [book:cli]. A plugin's standing `instructions` and any `instruct` text for this start go out as MCP server instructions; each tool description still stands alone [docs:plugin-api] [decision 130]. With no plugin host the list is empty. That is the contract before plugins load [docs:architecture] [person 2026-10-08].
- **Refused when**: a missing thread — `not-found` [op/].

### `CallTool`

- **Allowed when**: the named tool is offered to that thread [book:cli]. Its input is checked against the tool's schema before the plugin runs [docs:plugin-api].
- **Shows**: the tool's text, and whether that reply is an error [book:cli]. A tool or CLI job that runs past 120 seconds fails with an error that names the plugin [docs:plugin-api].
- **Refused when**: an unknown tool, including when no plugin host is installed — `not-found` [docs:architecture] [op/]. A schema miss — `invalid` [docs:plugin-api] [op/]. That empty host is the contract before plugins load [person 2026-10-08].
- **Writes**: `tool.started` before the dispatch and `tool.ended` after it, with one id of kagan's own; `skill_load` is served here and gets both too. `error` for a refusal, a schema miss, a plugin error, or a timeout, `ok` otherwise. A hidden thread gets both rows, and a missing thread neither; neither row holds the input, the reply, an error text or a plugin id [decision 154].

### `Details`

- **Allowed when**: the thread exists and has a folder [decision 111].
- **Changes**: nothing. It never fetches, never writes to the repository, and runs no `git status` [decision 111].
- **Shows**: the folder, the environment kind, the program line, and, in a git folder, the branch (none on a detached HEAD), the base branch with how many commits the branch is ahead of and behind it (none when the repository has no base branch), and the last five commits with short revs, newest first. Outside git, `git` is none [decision 111].
- **Refused when**: a missing thread — `not-found`. A thread with no folder — `invalid` [op/].

### `Links`

- **Allowed when**: the thread exists [decision 112].
- **Changes**: nothing.
- **Shows**: every http and https link the thread's terminals keep, as plain text, the program's terminal first and then each extra terminal in number order, each link once at the last place it was printed [decision 112].
- **Refused when**: a missing thread — `not-found` [op/].

### `Search`

- **Allowed when**: always [op/].
- **Shows**: threads whose title, or whose kept terminal lines, hold the text, ignoring case: threads that are not archived first, then archived ones when `--archived` is given, each marked `archived`. Hidden threads are left out. Each has its project's name and, when a kept line matched, the last such line, cut around the match, and the lines to scroll back to show it. At most `--limit`, 50 when left out [decision 107]. Kept lines are the running program's scrollback, or the screen kept after it ended; an archived thread's screen is released when its grace ends, so then only its title is searched [decision 107] [decision 26].
- **Refused when**: blank text, text longer than 256 characters, or a limit outside 1 to 200 — `invalid` [op/].

## Automation

An automation is a named text, a program line, and a time, that starts a thread in one project [CONTEXT] [decision 236]. The time is an interval of 1 to 1440 whole minutes, or one time to run once. It does not run while kagan is quit. The project page lists each automation and the thread each run started [decision 236].

### `Add`

- **Allowed when**: the project exists, the program line parses, and the time is either whole minutes from 1 to 1440 or one non-negative time [op/] [decision 236].
- **Changes**: saves the automation on that project. An interval is first due one interval after it is saved. A one-time automation is due at its time. The thread runs in the project folder [decision 236].
- **Shows**: the automation [book:cli].
- **Refused when**: an empty name, a name of only control characters, a name over 120 characters, empty text, text over 1 MiB, an empty program, an unfinished quote, minutes outside 1 to 1440, a negative time, both an interval and a time, or neither — `invalid` [op/]. A missing project — `not-found` [op/].

### `Pause`, `Resume`

- **Allowed when**: the automation exists [book:cli].
- **Changes**: a paused automation does not start another thread. A thread it already started keeps running. Resume lets a due automation start a thread again at once, so a pause longer than one interval starts one thread, not one for each missed interval [decision 236].
- **Shows**: the automation [book:cli].
- **Refused when**: a missing automation — `not-found` [op/].

### `Delete`

- **Allowed when**: the person approves the question in the app. Nothing is removed before that [decision 236].
- **Changes**: removes the automation. A thread it already started keeps running [decision 236].
- **Refused when**: a missing automation — `not-found` [op/]. A declined question — `invalid`, `declined: …` [op/].

### `List`

- **Allowed when**: always [book:cli]. One project when a project is set.
- **Shows**: each automation, and the thread each run started, oldest first. An interval that has not run yet has no thread [decision 236]. The window shows the run count and opens the last run; it does not list every run's title.

### Due run (no operation)

- **Allowed when**: kagan is open, the automation is not paused, its time is due, and no thread it started still has a running program [decision 236].
- **Changes**: starts a thread in that project with that program line, then sends the body through the same path as `ThreadOp::Send` once the program has started. Runs of one automation do not overlap. A thread already running is left alone. After a start, the next interval is one interval from that start, so time spent quit does not start a run for every missed interval. A one-time automation does not start another thread after one has been started. A run that does not start, or fails before its thread is saved, waits one more interval, or one minute for a one-time automation, and then tries again. A store read error is reported, and the next tick tries again. Opening kagan starts one run that became due while it was quit [decision 236].
- **Shows**: the thread on the automation, once a run has started [decision 236].

## Diagnostics

`kg diagnostics export` writes one new local file. Omit `--path` and it goes under the data directory, in `diagnostics/`. A given path is refused when the parent folder is missing or the file already exists. Each plugin's warn and error counts come from the plugin host, which reads the last 64 KiB of its log; without a plugin host the line says `not read, no plugin host`.

- **Changes**: creates that file. It names the kagan version, the schema version, and the data directory. Each thread gets its id, status, and structured cause, and whether a screen was kept. Each plugin gets its id, kind, version, and warn and error counts. Titles, end text, plugin sources, and plugin log messages are omitted. The file says a program that was still running when kagan quit has no saved screen.
- **Refused when**: the file already exists, or the folder for `--path` does not exist — `invalid` [op/]. The thread actions menu has the same export. The ended line has that button when the program failed, was interrupted, or its screen was not kept in full. A clean stop or an exit 0 does not. There is no default shortcut.

## Project

A project is a named body of work, usually one repository, with one folder on this machine. Its kind is `standard` or `personal` [CONTEXT]. The first launch creates the Personal project; its id is `personal` [book:start].

### `List`, `Get`

- **Allowed when**: `Get` needs a project id [book:cli]. `List` always [book:cli].
- **Shows**: the projects, or one project [book:cli]. Standard projects follow the person's order; Personal is last in the sidebar [docs:look] [decision 132].
- **Refused when**: a missing id on `Get` — `not-found` [op/].

### `Add`

- **Allowed when**: the command is given a folder path [book:cli] [op/].
- **Changes**: adds a `standard` project for that folder [book:cli] [person 2026-10-08]. The book also says this version has no such command; the CLI has it, so Add is in the contract [book:threads] [book:cli]. A new project is appended after the other standard projects [decision 132].
- **Refused when**: a path the command cannot use as a project folder — `invalid` [op/].

### `Reorder`

- **Allowed when**: the id is a standard project [book:cli] [decision 132].
- **Changes**: moves that project before `--before`, or to the end of the standard list when `--before` is omitted [book:cli] [decision 132]. Dragging a project header in the sidebar does the same [docs:look].
- **Refused when**: a missing id — `not-found` [op/]. Personal, or `--before` naming Personal — `invalid` [op/] [decision 132].

### `SetProgram`

- **Allowed when**: the id is a live project [op/]. A program, when given, splits as a command line [op/].
- **Changes**: sets the program a new thread in that project starts with, or clears it when no program is given [decision 221]. The program is used only while it is installed; otherwise the first installed program is the default [decision 221].
- **Refused when**: a missing id — `not-found` [op/]. A program that does not split, such as one with an unfinished quote — `invalid` [op/].

## Program

### `List`, `Discover`

- **Allowed when**: always. With `--project`, the project's program is read as the first recent program, so it is the default when it is installed [decision 221].
- **Shows**: the known programs, the shell, and the recent programs, with the default marked [book:cli]. An installed known program is a card with a letter mark, its short name and its version; a missing one is a plain row marked Not installed, never a card. A recent command is a row named by its program's short name, with its arguments under it; the whole command is its tooltip. The shell is a row with its path and no mark [decision 237].
- **Refused when**: `--project` names no live project — `not-found` [op/].

## Env

An environment variable is a name and a value that every program kagan starts gets: the global ones, and the ones set for one project, which win [decision 220]. A program started before a change keeps what it had. Setup and teardown scripts (`.kagan/setup`, `.kagan/teardown`) get the same variables, global then project [decision 220].

### `List`

- **Allowed when**: `--project` names a live project [op/].
- **Shows**: the names only, for the global scope or that project [decision 220]. A value is never returned.

### `Set`

- **Allowed when**: the name is a shell name that does not start with `KG_` and is not `PATH`, in any case. The value is one line, not empty, and at most 1024 bytes. `--project` names a live project [op/] [decision 220].
- **Changes**: stores the value under that name, replacing one that was there [decision 220]. The value comes from stdin, or from a prompt that does not show it, never from the command line [book:cli].
- **Refused when**: a name or value that breaks the rules above — `invalid`, and the message never repeats the value [decision 220]. A request that names a thread (kagan's own terminals do) — `invalid`, which points to Settings [decision 220]. A missing project — `not-found` [op/].

### `Remove`

- **Changes**: forgets that name in that scope [decision 220]. A request that names a thread is refused, as for `Set` [decision 220]. A values file that cannot be read is moved aside, and the result is a warning [decision 220].
- **Refused when**: the name is not set in that scope — `not-found` [op/].

## Environment

An environment is the folder a thread works in, plus a status (`creating`, `provisioning`, `ready`, `error`, `destroyed`) [CONTEXT]. The providers are project, worktree, and personal [decision 2] [CONTEXT]. A project checkout is unmanaged and is never removed [CONTEXT] [decision 27]. A worktree is managed: kagan removes it a short grace after the last unarchived thread using it is archived, and it can still be reused while it is retiring [CONTEXT] [decision 26]. A personal environment is kept through archive and restart; only Delete removes it, after a person approves [decision 27].

### `Get`, `Branches`, `Setup`

- **Allowed when**: the id is known [book:cli]. `Branches` and `Setup` take a project [book:cli]. `Get` takes an environment [book:cli].
- **Shows**: `Get` the environment, including the removal reason when git refused to delete the worktree [book:cli] [decision 28]. `Branches` the local branch names, which one is current, and why the project folder cannot change branches [book:cli] [decision 170]. `Setup` the commit a revision names, and the setup script at it: its kind (`link` or `script`), or none when the commit has no setup file or its file mode is not one kagan knows, its text cut to 40 lines and 2,000 characters, and whether it was cut. None overall when the revision does not name a commit (decision 189). `changes`, `pull-request` and `open` were removed as unused (decision 36).
- **Refused when**: a missing id — `not-found` [op/]. A revision that starts with `-` — `invalid` [op/].

### `Restore`

- **Allowed when**: the thread had a managed environment whose folder was removed cleanly [book:threads] [decision 28].
- **Changes**: attaches the folder again from the branch that was left in place [book:threads] [decision 28]. A dirty worktree is never force-removed. A removal git refused because of files it would track stays `error`, names those files, and offers Remove again. Restore of that folder clears the reason and marks it `ready`. Restore stays available for a folder that was removed [decision 28] [decision 64] [docs:look]. A half-made managed environment that a restart finds with no thread is destroyed instead of restored [docs:architecture].
- **Shows**: the environment [book:cli].
- **Refused when**: a missing thread — `not-found` [op/]. A folder that cannot be attached — `failed` [op/].

### `Remove`

- **Allowed when**: a worktree whose removal git refused, so the environment is `error` and `Get` includes the reason [decision 28] [decision 64].
- **Changes**: runs teardown, then `git worktree remove` with no `--force` [decision 28]. Files git ignores go with the folder. Files git would track stay, and the reason names them [decision 64].
- **Shows**: done when the folder is gone [book:threads]. The refusal, when the folder is still there — `failed` [decision 28].
- **Refused when**: an environment that is not waiting on that refusal — `invalid` [op/].

### `SetBase`

- **Allowed when**: the environment exists [book:cli].
- **Changes**: the branch that change counts compare against [book:cli].
- **Shows**: the environment, with that base stored [book:cli]. The window does not name the base in a strip.
- **Refused when**: a missing environment — `not-found` [op/].

## Provider

The provider on a thread is the program line that thread runs, such as `claude` or `codex` [CONTEXT]. There is no registry and no install of agent programs.

## Plugin

A plugin is a package installed from git that adds UI, commands, tools, or skills [CONTEXT]. A plugin's thread record omits the program line [docs:plugin-api]. There is no trust shortcut for a plugin an agent built [docs:self-extend]. This contract has no catalog and no marketplace command (the window's five suggested plugins are a shipped list, not an index [decision 217]); install is a git link or a local folder [docs:self-extend] [book:plugins]. Decision 3 describes a later index and is not an Op here [decision 3]. A manifest with `"areas": ["audio"]` is refused as an unknown area [decision 42]. When loaded, its script reads its data folder as `process.dataDir` [decision 127]. A plugin calls another's `exports` only when the caller lists that plugin in `powers.calls` and the callee lists the name; the call waits at most 30 s [decision 155] [docs:plugin-api].

### `List`, `Logs`, `Types`

- **Shows**: `List` the installed plugins [book:plugins]. `Logs` its log, crashes, and slow marks; `kg` implements `--follow` by repeating the read, so the op itself never follows [book:plugins] [docs:self-extend]. The log keeps the current file and one previous file, each at most 10 MiB; a cursor past the end returns `reset` and starts at 0; a record larger than 10 MiB is rejected [docs:self-extend]. `Types` rewrites the declaration files for this version [docs:self-extend].
- **Refused when**: a missing plugin id — `not-found` [op/]. A log record larger than 10 MiB — `invalid` [docs:self-extend] [op/]. Any other log or types failure — `failed` [op/].

### `Cli`

Not in `kg plugin` [book:cli]. It runs a command the plugin declares under `cli` [docs:plugin-api].

- **Shows**: exit code, stdout, and stderr [docs:plugin-api]. The project is the thread's project, else the project whose folder contains the working directory [docs:plugin-api].
- **Refused when**: the job runs past 120 seconds — `failed` [docs:plugin-api] [op/]. With no plugin host, the operation is unavailable — `failed` [docs:architecture] [op/]. That is the contract before plugins load [person 2026-10-08].

### `Command`

- **Allowed when**: the plugin declares the command, and its `when` matches: `always`, a thread on screen, or a project [docs:plugin-api] [book:cli].
- **Changes**: runs that command [book:cli]. The job has no reply body [docs:plugin-api]. A command with `"header": true` and `"when": "thread"` is also a button on the shown thread's title bar; a click runs the same path [decision 129].
- **Refused when**: past 10 seconds — `failed` [docs:plugin-api] [op/]. Unavailable without a plugin host — `failed` [docs:architecture] [op/]. That is the contract before plugins load [person 2026-10-08].

### `New`

- **Changes**: scaffolds a plugin folder: manifest, entry, and declaration files [docs:self-extend]. `--dir` chooses the folder [book:cli].
- **Refused when**: a bad name — `invalid` [op/].

### `Check`

- **Changes**: nothing. It checks the manifest (unknown keys, schemas, declared views exported) and the scripts (a host import the module does not export, a `gpui-component` state constructor the schema rejects, a component given the `gpui-base` state of the same name). It does not run the plugin [docs:self-extend] [docs:plugin-api].
- **Refused when**: an invalid manifest — `invalid` [op/]. An unknown `powers.areas` entry, including `audio` and `downloads` — `invalid` [decision 42] [decision 65] [op/]. A `downloads` key or `powers.spawn` is refused [decision 65]. A script the host would throw on at load or when a panel opens — `invalid` [decision 173] [docs:plugin-api].

### `Reload`

- **Allowed when**: the plugin is installed [book:cli]. A person approves when powers grew [decision 14] [docs:self-extend]. A `downloads` key or `powers.spawn` is refused [decision 65].
- **Changes**: reloads the installed code [docs:self-extend]. Unload cancels the plugin's tasks, closes its job queue, fails its open jobs, kills processes it started, and releases its port [docs:plugin-api].
- **Refused when**: the person declines — `failed` [docs:self-extend] [book:plugins] [op/].

### `Schedule` (a manifest entry, no operation)

- **Allowed when**: the plugin is enabled and its manifest declares `schedules` with `every` from 1 to 1440 whole minutes [docs:plugin-api] [decision 146].
- **Changes**: runs the plugin's `schedule` job every interval while kagan is open, loading the plugin for the run if needed. Runs never overlap [docs:plugin-api].
- **Refused when**: an interval out of range or a duplicate id — the manifest is refused at install and check [docs:plugin-api]. A run that fails or passes 2 minutes is logged to the plugin log; a timeout retires that load so the next run starts a new one, and the next wait still starts.

### `Dev`

- **Allowed when**: the path is an installed local plugin [docs:self-extend].
- **Changes**: watches the folder and reloads on each change; there is no build step [docs:self-extend]. Each reload uses the same approval as `Reload` [docs:architecture].
- **Refused when**: a declined approval — `failed` [docs:architecture] [book:plugins] [op/].

### `Remove`

- **Allowed when**: a person approves [decision 12] [book:plugins].
- **Changes**: removes the installed plugin [book:plugins]. Nothing is removed until they approve; declining ends the command [docs:plugin-api].
- **Refused when**: the person declines — `failed` [book:plugins] [op/]. An agent cannot answer [decision 10].

### `Install`

- **Allowed when**: the source is `git:<url>@<ref>` or a local folder [book:plugins]. A person approves the powers [decision 10] [book:plugins].
- **Changes**: installs that commit [decision 14]. A local folder is linked, not copied [book:plugins] [docs:self-extend]. `--plugin` names which plugin in a repository [book:cli]. The install screen shows powers and folder settings in plain words [docs:plugin-api]. A `downloads` key or `powers.spawn` is refused [decision 65]. `powers.read` entries must be folders under the home directory; the data directory, `~/.ssh`, and `~/.gnupg` are refused, and so is a path that escapes home [docs:plugin-api] [decision 44]. An older app than the manifest's `kagan` version refuses [docs:plugin-api].
- **Shows**: the powers question, then the installed plugin [book:plugins].
- **Refused when**: the person declines — `failed` [book:plugins] [op/]. A manifest the check would reject — `invalid` [docs:plugin-api] [op/].

### `Update`

- **Allowed when**: the plugin was installed from git [book:plugins] [docs:plugin-api]. With `--all`, each git plugin is fetched in turn; one whose update adds powers is not moved and is listed under `needs_approval`, so the person approves it alone [decision 230].
- **Changes**: moves to the latest commit [book:cli]. The same code keeps running until then [book:plugins]. An update whose powers grew asks again [decision 14]. A `downloads` key or `powers.spawn` is refused [decision 65]. With `--all`, each plugin is updated in turn; the result names the plugins that moved, each that needs approval, and one line for each that failed, and a failure does not stop the others [decision 230].
- **Refused when**: the person declines a wider update — `failed` [decision 14] [book:plugins] [op/]. Neither a plugin id nor `--all`, or both — `invalid` [op/].

### `Outdated`

- **Shows**: each git plugin whose ref now names a different commit from the one installed: its id, name, the installed commit and the latest one. A ref no branch or tag names, that the installed commit starts with, is a commit pin and is skipped, because it cannot move [decision 230]. A plugin that cannot be read is named with its error, and the others are still checked. Nothing is installed, moved or kept [decision 230]. The check asks the remote with `git ls-remote`, so it does not download a plugin; an annotated tag is compared by the commit it names. A git error shows no credential from the source URL [decision 230].
- **Refused when**: never.

### `Enable`

- **Allowed when**: the plugin is installed and disabled [book:plugins].
- **Changes**: loads it on next use [docs:plugin-api] [book:plugins]. Discover registers contributions without running the script; load runs on first use [docs:plugin-api].
- **Refused when**: a missing id — `not-found` [op/].

### `Disable`

- **Changes**: the plugin is not loaded [book:plugins]. A plugin whose renders stay over 16 ms is marked slow and the row offers disable; it is not unloaded on its own [decision 25].
- **Refused when**: a missing id — `not-found` [op/].

### `Config`

- **Changes**: reads, writes, or clears one plugin setting, through `Config.Get`, `Config.Set` and `Config.Clear` below [docs:plugin-api] [book:cli]. A folder setting is approved on the calling thread [book:cli].
- **Refused when**: a declined folder approval — `failed` [book:plugins] [op/].

### `Use`

- **Changes**: saves which plugin fills a slot, `threads` or `review`, as the preference `slot.<slot>`; `kagan` is the built-in. The window switches at once (decision 105) [op/].
- **Refused when**: the plugin is not enabled or offers no panel that replaces that slot — `invalid` [op/].

## Skill

A skill is a folder with a `SKILL.md`: a lowercase hyphenated `name` that matches the folder, a `description`, then the instructions [docs:self-extend] [book:skills]. Sources, lowest first: bundled, plugin, user (`<dataDir>/skills`), project (`<project>/.kagan/skills`) [docs:self-extend]. A higher source overrides a lower one with the same name [decision 24]. Two skills with the same name in one source are both dropped, and `List` shows the error [decision 24]. A user or project skill whose name starts with `kg-` is dropped with an error; that prefix is reserved for bundled and plugin skills [decision 158]. A plugin's skills are the folders its record holds: set on install, update, reload, and at each app start, not read from `kagan.json` on each list. Every start with agent integration writes the resolved kagan skills (bundled, plugin, user, project) to an immutable per-content catalog, and exposes it natively to the known programs. `List`, `Show` and `Files` read the four kagan sources [decision 203]. When a running plugin declares a `kg` command, `kg-plugin-commands` is generated from the running plugins on each read and listed as bundled; it is absent otherwise [decision 209].

### `List`

- **Shows**: the visible skills and the per-source duplicate errors [decision 24] [book:skills]. `--project-id` picks the project skills [book:cli].
- **Refused when**: nothing in the sources names a hard failure. The duplicate case is reported in the result, not as a refusal [decision 24].

### `Show`

- **Allowed when**: one skill has that name after overrides [book:skills].
- **Shows**: the skill and its body [book:cli] [docs:self-extend].
- **Refused when**: an unknown name, including one dropped as a duplicate in its source — `not-found` [op/] [person 2026-10-08].

### `Files`

- **Shows**: the skill's file paths. A bundled skill's files are real files under `<dataDir>/agent/bundled/<hash>` [decision 208] [docs:self-extend] [book:skills].
- **Refused when**: an unknown name — `not-found` [op/].

### `New`

- **Changes**: writes a skill folder into user skills, or into the project when `--project` is set [book:skills] [docs:self-extend]. Its SKILL.md starts from `--template` (`blank`, `review-pr`, `release-notes`, or `debug-test`; `blank` by default) [decision 216].
- **Refused when**: a name that is not lowercase and hyphenated, starts with `kg-`, or already exists in that source — `invalid` [docs:self-extend] [op/] [person 2026-10-08].

### `Add`

- **Changes**: copies an existing skill folder into user skills, or into the project when `--project` is set [book:skills].
- **Refused when**: a path that is not a skill folder, or a skill name that starts with `kg-` — `invalid` [op/].

### `Open`

- **Changes**: nothing on disk. It asks the window to show the Skills page (`kg skill open`). If no window is open, it opens one [decision 205].
- **Refused when**: never.

### `Update`

- **Allowed when**: the skill is a user or project skill [decision 203].
- **Changes**: replaces the whole `SKILL.md` with the text given, frontmatter included. The file is written as-is, unless it has changed since the window read it: then nothing is written and the call is refused [decision 206]. `kg skill update` does not send that check [decision 206].
- **Refused when**: an unknown name — `not-found` [op/]. A bundled or plugin skill — `invalid`; or text whose frontmatter does not parse, or names another skill — `invalid` [decision 203]; or the file changed since the window read it — `invalid` [decision 206].

### `Remove`

- **Allowed when**: the skill is a user or project skill [book:skills] [docs:self-extend].
- **Changes**: deletes that skill folder [book:skills]. A folder that is a link is not listed, so Remove does not find it [decision 206].
- **Refused when**: an unknown name — `not-found` [op/]. A bundled or plugin skill — `invalid` [book:skills] [op/] [person 2026-10-08] [decision 203].

## Section

A section is a user-named group that threads can be filed under in the sidebar [CONTEXT].

### `Create`

- **Changes**: creates a section, and files the given thread under it when a thread id is passed [book:cli].
- **Refused when**: an empty name, or a name that already exists — `invalid` [person 2026-10-08] [op/].

### `Rename`

- **Changes**: renames the section [book:cli].
- **Refused when**: a missing section — `not-found` [op/]. An empty name — `invalid` [person 2026-10-08] [op/].

### `Delete`

- **Changes**: removes the section. Threads in it stay, unfiled [book:cli].
- **Refused when**: a missing section — `not-found` [op/].

### `File`

- **Allowed when**: the thread exists, including an archived thread [book:cli] [person 2026-10-08].
- **Changes**: files the thread under the section, or clears the section when the id is omitted [book:cli].
- **Refused when**: a missing thread or section — `not-found` [op/].

## Saved prompt

A saved prompt is stored text the person can send, with a title, a body, and a starred flag. It is not a question waiting for an answer [CONTEXT] [decision 234]. Rows live in SQLite. Add, star, and delete are serialized. Search and send are not, and send follows `ThreadOp::Send` [decision 234].

### `Add`

- **Allowed when**: the title trims to at most 200 characters with no control character, and the body has text other than control characters and is at most 1 MiB [op/].
- **Changes**: stores the trimmed title, the body as given, and starred off. The id starts with `svp_` [decision 234].
- **Refused when**: an empty title or body, a body of only control characters, a longer title, a control character in the title, or a body over 1 MiB — `invalid` [op/].

### `Search`

- **Allowed when**: the text is at most 256 characters. Empty text lists every saved prompt [op/].
- **Shows**: saved prompts whose title or body contains the text, ignoring case for ASCII letters only. A `%`, `_`, or `\` in the text is literal. Starred rows come first, then title, then id [decision 234].

### `Star`

- **Changes**: sets the starred flag [decision 234].
- **Refused when**: a missing id — `not-found`. An id that is not a plain identifier — `invalid` [op/].

### `Delete`

- **Changes**: removes the saved prompt [decision 234].
- **Refused when**: a missing id — `not-found`. An id that is not a plain identifier — `invalid` [op/].

### `Send`

- **Allowed when**: the saved prompt exists and `thread_id` names a thread. The CLI reads `thread_id` from `KG_THREAD_ID` when it is not given. The window passes the thread it shows, and shows "No thread is shown." when Settings covers it [decision 234].
- **Changes**: types the body into that thread and presses Enter, through the same path as `ThreadOp::Send` [decision 234].
- **Refused when**: a missing saved prompt — `not-found`. A missing thread — `not-found`. An id or thread id that is not a plain identifier — `invalid`. The same refusals as `ThreadOp::Send` [op/].

## Diff

A thread environment's diff reads git and never writes the worktree [book:cli]. Scopes are uncommitted, the branch against a base, and one commit [docs:look] [book:cli].

### `List`

- **Allowed when**: the thread has an environment git can read [book:cli]. `branch` may name a base; `commit` may name a rev [book:cli].
- **Shows**: the changed files in that scope [book:cli]. Each row shows the kind, the path, and the counts. Nothing marks a file as the agent's (decision 60).
- **Refused when**: a missing git or a missing base — `failed` [docs:architecture] [op/].

### `Show`

- **Allowed when**: the path is in that scope [book:cli].
- **Shows**: that file's diff and its new side [book:cli]. Opening it shows the review page in the center: the hunks at once, read-only (decision 60).
- **Refused when**: the same git errors as `List` — `failed` [docs:architecture] [op/].

### `Commits`

- **Shows**: commits that can be reviewed, newest first [book:cli].
- **Refused when**: a missing git — `failed` [docs:architecture] [op/].

## Preview

One file, read-only, in the Preview tab of the right sidebar [docs:look] [decision 149]. The window shows the file on screen from a review, or one the person picks; `kg preview show` shows it in the tab and opens the named thread first [book:cli]. Nothing is edited, saved, searched or fetched, and there is no plugin entry (decision 70).

### `Show`

- **Allowed when**: a thread names the file, in which case a relative path joins that thread's environment folder and the resolved path must stay inside it; with no thread the path must be absolute [decision 149]. A folder, a missing file and a special file are refused — `invalid` and `not-found`. `--thread-id` defaults to `KG_THREAD_ID` [op/].
- **Shows**: `path`, the resolved `absolute` path, `bytes` and one kind: text, Markdown, CSV/TSV, an image's format and pixel size, or one sentence saying why nothing is shown [decision 149]. Extension picks Markdown and CSV; PNG and JPEG magic bytes pick the image; any other UTF-8 file is plain text [decision 149]. Text and Markdown stop at 1 MiB, 2,000 lines and 4,000 bytes a line; CSV/TSV also at 100 rows, 20 columns and 120 characters a cell; images at 8 MiB and 40 megapixels. A NUL byte in the first 8 KiB is binary and invalid UTF-8 is refused, never guessed; a UTF-8 BOM is dropped and CRLF becomes LF. A file past a cap shows no content and says so in one sentence [decision 149].
- **Refused when**: the path leaves the thread's folder, or a relative path has no thread — `invalid` [decision 149]. An empty path or one with a control character is refused by `check` [op/].
- **Notes**: an image's bytes never travel in the reply; the window reads the path again, canonicalizes it, and checks it against the thread's folder before drawing (decision 149).

## Ui

`kg ui` drives the native window without the operating system's mouse or keyboard, including while the window is in the background [docs:ui-driver]. Input goes through the window's own event dispatch [docs:ui-driver] [docs:architecture]. `--window` is a zero-based index into open windows; the default is 0 [docs:ui-driver]. The app must already be running with a window; `Open` installs the driver [docs:ui-driver]. Synthetic input cannot answer a person-only approval [decision 10] [docs:ui-driver].

### `Snapshot`

- **Allowed when**: a window is open [docs:ui-driver].
- **Shows**: the observed elements: stable ids, role, label, value, bounds, focused, disabled, visible, and hint [docs:ui-driver] [book:cli].
- **Refused when**: no driver — "the ui driver needs a kagan window" [docs:ui-driver] — `failed` [op/].

### `Click`

- **Changes**: a left click at the element center [docs:ui-driver].
- **Refused when**: a missing element — `not-found` [op/]. A click that would answer a person-only approval returns "person-only approvals cannot be answered with kg ui" [docs:ui-driver] — `failed` [op/].

### `Type`

- **Changes**: sends the text to the focused control [docs:ui-driver].
- **Refused when**: the person-only message above, when the keys would answer [docs:ui-driver] — `failed` [op/].

### `Key`

- **Changes**: sends a keystroke such as escape or a shortcut [docs:ui-driver] [book:cli].
- **Refused when**: the person-only message above [docs:ui-driver] — `failed` [op/].

### `Scroll`

- **Changes**: scrolls the element by `dy` pixels [docs:ui-driver].
- **Refused when**: a missing element — `not-found` [op/].

### `Screenshot`

- **Shows**: a PNG at the given path, without focusing the window [docs:ui-driver] [book:cli]. A covered window captures the last presented buffer [docs:ui-driver]. An offscreen window renders its own scene, so the capture is current [decision 238].
- **Refused when**: a platform without capture — `failed` [docs:ui-driver] [op/]. An offscreen window on a platform without offscreen capture — `failed`, `screenshot unavailable` [decision 238]. No window — `failed` [docs:ui-driver] [op/].

## Browser

`kg browser` gives the thread named by `KG_THREAD_ID` the Browser tab's one page [decision 143]. One browser session exists per window, owned by that thread; another thread is refused and the message names the owner. `click`, `fill`, `type`, `press`, `select` and `scroll` act on that page through fixed host scripts [decision 147]. On Windows the agent page has WebView2's default script dialogs off, and on Linux every page dismisses script dialogs, so a page's `alert`, `confirm` or `prompt` cannot stall a command; macOS shows no script dialogs. The page is private, as the person's is: only http and https, no camera or microphone, no new windows, no downloads [decision 117]. The first command after 30 idle minutes ends the browser session and fails with `the browser session ended after 30 idle minutes`, and the page stays as the person's. Stop agent, `close`, closing the tab, closing the window, or quitting ends it; the page then belongs to the person again. Showing another thread hides the page but does not end the browser session, and the Browser tab comes back with the thread. A Linux window that is not on X11 fails with `The browser needs an X11 window. On Wayland, start kagan where DISPLAY is set (XWayland).` [decision 164]. A plugin with the browser area calls the same verbs through `kagan/browser` and names the thread on each call [decision 161].

### `Open`

- **Allowed when**: the caller owns the browser session, or no browser session exists [decision 143].
- **Changes**: takes the Browser's one page and loads the URL. macOS and Windows drop the person's page; Linux loads the new address in the same native page [decision 164]. A bare name gets https, a local host gets http, and anything that is not http or https is refused [decision 117]. It shows the tab, opens the right sidebar, and selects it only when the caller is the thread on screen, and it never takes the keyboard [decision 143]. Another thread's browser session is left alone.
- **Shows**: the driven page: thread id, thread title, url, title, and whether the tab is drawn.
- **Refused when**: an empty or non-web address — `type a web address, such as example.com` — `invalid` [op/]. Another thread's browser session — `thread <title> is driving the browser; close it with kg browser close` — `invalid`. No window — `browser control needs a kagan window` — `failed`. A Linux window that is not on X11 — `The browser needs an X11 window. On Wayland, start kagan where DISPLAY is set (XWayland).` — `failed` [decision 164].

### `Status`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Shows**: the same fields as `Open`, read from the page.
- **Refused when**: no browser session — `no browser session; run kg browser open <url> first` — `not-found`. Another thread's browser session — `invalid`, with its title. A page that does not answer within 10 seconds — `failed`.

### `Snapshot`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Shows**: the url, title, each control with a ref and its role, name, value, focused, disabled and checked, then the page text [decision 145]. Refs are numbered from 1 in document order and are valid until the next snapshot or a navigation. Links, buttons, inputs other than hidden ones, selects, textareas, summaries, editable elements, `[tabindex]`, elements with a role, and headings are listed; an iframe is named and not inspected. Hidden elements, `aria-hidden` elements and hidden inputs are left out. A password control never reports a value: it is an `input` whose type is `password`, whatever role it carries, and the host drops a value for it even if the page sends one. At most 300 controls, 120-character names, 200-character values, 8 KiB of text and 24 KiB of JSON; when cut, the snapshot says so and the text ends with `… N more controls not shown`, or `… text cut at the size limit` when the JSON cap shortened the text. Text is what a person can read on the page, so it can contain anything visible [decision 145].
- **Refused when**: no browser session or another thread's browser session — as `Status`. A page that does not answer within 10 seconds — `the page did not answer within 10 seconds` — `failed`. A reply that is empty or not JSON — `the page could not be read; take the page over or try again` — `failed`.

### `Goto`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Changes**: loads another URL in the page, under the same address rule as `Open`, and waits for the new document. Old refs are gone.
- **Shows**: done.
- **Refused when**: an empty or non-web address — `invalid`. No browser session, another thread's browser session, or no window — as `Open` and `Status`.

### `Click`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Changes**: focuses the control with that ref, then dispatches `pointerdown`, `mousedown`, `pointerup`, `mouseup` and `click`, all bubbling, cancelable and composed, so the page's default action runs [decision 147]. A `file` input is refused before any event, because a chooser would open on the person's desktop.
- **Shows**: done.
- **Refused when**: a ref that is not on the page — `ref N is not on the page; take a fresh snapshot` — `not-found`. A `file` input — `a page cannot choose a file; the person uploads it` — `invalid`. No browser session, another thread's browser session, or no window — as `Status`.

### `Fill`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Changes**: focuses the control with that ref, then writes the text through the prototype's `value` setter and dispatches `input` and `change`; a `contenteditable` uses `insertText` [decision 147]. `--file -` reads the text from stdin, and `--file PATH` reads it from a file of at most 1 MiB; a password never appears in a result, an error or the snapshot. A password field may be filled. A page that throws during the write is reported as `the page refused the text`, without its message. A `file` input is refused before any event.
- **Shows**: done.
- **Refused when**: text and `--file` together — `pass the text or --file, not both` — `invalid`. A path sent over the socket in place of text — `send the text, not a path` — `invalid`. Text over 1 MiB or not UTF-8 — `invalid`. A dead ref — `not-found`. A `file` input — `invalid`. No browser session, another thread's browser session, or no window — as `Status`.

### `Type`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Changes**: inserts the text into the focused control the way `Fill` writes one, at the cursor where the engine can, and does nothing when no control has focus [decision 147]. A page that throws during the write is reported as `the page refused the text`, without its message.
- **Shows**: done.
- **Refused when**: empty text — `text must not be empty` — `invalid`. Text over 1 MiB — `invalid`. No browser session, another thread's browser session, or no window — as `Status`.

### `Press`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Changes**: dispatches `keydown`, `keypress` and `keyup` on the focused control with `key`, `code` and `keyCode` for `enter`, `escape`, `tab`, `space`, `backspace`, `delete`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, or one printable character [decision 147]. `Tab` moves focus. `Enter` presses a submit button when the form has one, else calls `requestSubmit` when the engine has it, else calls `submit`, which skips validation and the submit event; it does both only when neither the keydown nor keypress event was canceled, so a page that submits on Enter without canceling can submit twice. `Enter` on a link does nothing.
- **Shows**: done.
- **Refused when**: an empty key — `key must not be empty` — `invalid`. Any other name or more than one character — `invalid`, with the accepted names. No browser session, another thread's browser session, or no window — as `Status`.

### `Select`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Changes**: chooses the `<select>` option whose value or label matches, then dispatches `input` and `change` [decision 147].
- **Shows**: done.
- **Refused when**: no matching option — `invalid`, listing up to ten labels. A ref that is not a `<select>` — `invalid`. A dead ref — `not-found`. No browser session, another thread's browser session, or no window — as `Status`.

### `Scroll`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Changes**: scrolls the page instantly by `dy` pixels, positive for down [decision 147].
- **Shows**: done.
- **Refused when**: a value that is not a number — `invalid`. No browser session, another thread's browser session, or no window — as `Status`.

### `Back`, `Forward`, `Reload`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Changes**: moves through the page's history, or loads it again. `Reload` waits for the new document.
- **Shows**: done.
- **Refused when**: no browser session, another thread's browser session, or no window — as `Status`. The page could not run the history step — `failed`.

### `Close`

- **Allowed when**: this thread owns the browser session [decision 143].
- **Changes**: ends control and leaves the page as the person's, with its last address in the address field. The browser session is gone at once, so the next command is `not-found` [decision 143].
- **Shows**: done.
- **Refused when**: no browser session — `no browser session; run kg browser open <url> first` — `not-found`. Another thread's browser session — `invalid`.

## Theme

The app starts on `fjord`, in system mode, so it follows the operating system's light or dark mode [book:themes]. Modes are `light`, `dark` and `system` [book:themes]. A plugin does not add a theme; the manifest key is accepted and ignored [docs:self-extend]. Core stores the id and the mode; the window owns the files and the colors [docs:architecture].

### `Get`

- **Shows**: the selected theme id and mode [book:themes] [docs:look].

### `Set`

- **Allowed when**: the id is lowercase letters, digits, and dashes, and the mode is `light`, `dark` or `system` [book:themes] [docs:look]. `fjord`, `ember` and `grey` ship with the app [book:themes].
- **Changes**: selects that theme and mode for every open window [docs:look] [book:cli]. A custom theme is `<dataDir>/themes/<id>.json` [book:themes]. A well-formed id with no file yet stays selected; the window shows an error and uses the default palette for the mode [book:themes] [docs:look]. Saving a file updates the list; a missing or invalid selected file does the same and keeps the id [book:themes] [docs:look].
- **Refused when**: an id with any other character, a path, a dot, or traversal — `invalid` [docs:look] [op/].

## Keymap

Built-in shortcut overrides, and the keys of plugin commands, named `<plugin-id>:<command>` [book:cli] [decision 102]. On macOS the shortcut key is Command. On Windows and Linux a `secondary` chord is Control and Shift together, so a bare Control chord stays with the program [book:window]. One data directory is one machine, so a saved chord is not stored per platform. `kg keymap set --platform` was removed as unused (decision 54).

### `Get`

- **Shows**: the saved overrides under `shared`, and under `plugins` each enabled plugin command with its key, or null when it has none [decision 102]. Defaults, before an override, are NewThread `secondary-n`, StopThread `secondary-.`, ArchiveThread `secondary-shift-backspace`, TogglePinned `secondary-shift-p`, OpenSettings `secondary-,`, UseProjectFolder `secondary-alt-p`, UseWorktreeFolder `secondary-alt-w`, RetryBranches `secondary-alt-r`, RetryPrograms `secondary-alt-l`, ToggleCustomCommand `secondary-alt-c`, ThreadActions `secondary-shift-m`, ToggleSidebar `secondary-b`, ToggleRightSidebar `secondary-\`, AddProject `secondary-shift-o`, NextHunk `alt-]`, PrevHunk `alt-[`, FindInDiff `secondary-f`, NextMatch `secondary-g`, PrevMatch `secondary-alt-g`, StartAgain `secondary-shift-s`, FocusSidebar `secondary-shift-l`, RestoreThread `secondary-shift-r`, UseBuiltInSidebar `secondary-alt-b`, UseBuiltInReview `secondary-alt-v`, OpenCommandList `secondary-shift-a`, SearchThreads `secondary-alt-f`, OpenSavedPrompts `secondary-shift-i`, NewTerminal `secondary-t`, CloseTerminal `secondary-shift-w`, NextTerminal `secondary-alt-]`, PreviousTerminal `secondary-alt-[`, and OpenLocalLink `secondary-alt-o` [op/]. Bigger is `secondary-=`, Smaller `secondary--`, ActualSize `secondary-0`, NextThread `secondary-shift-]`, PreviousThread `secondary-shift-[`, NextThreadNeedingYou `secondary-alt-j`, Thread1 through Thread9 `secondary-1` through `secondary-9`, SendReview `secondary-enter`, and ToggleWrap `alt-z` [op/]. Off macOS a `secondary` chord is bound as `ctrl-shift` plus the rest, and a following `shift-` is already in that. A saved override is used as saved. ShowHelp, NextFile and PrevFile were removed as unused (decision 54). The command list came back as OpenCommandList (decision 107).

### `Set`

- **Allowed when**: the action is one of those defaults, or a command of an enabled plugin [op/] [book:cli] [decision 102].
- **Changes**: records the chord for this machine [book:cli]. `--reset` drops the saved chord and uses the default key [book:cli]. A conflicting chord is not stored; the result names the other action [op/]. A plugin command's key is refused when it overlaps a window key or another plugin command's key [docs:plugin-api]. It is stored with that plugin and applies while the app is in front [decision 102].
- **Refused when**: an unknown action, or a plugin that is not enabled or has no such command — `invalid` or `not-found` [op/]. An empty key or an unknown key name — `invalid` [op/].

### `Remove`

- **Changes**: leaves the shortcut with no key, until it is set or reset [decision 102] [decision 222]. For a built-in, the saved mark is an empty key; `Get` shows it as an empty string, and `Set` with `--reset` brings the default back [decision 222].
- **Shows**: in Settings → Keyboard, a removed built-in stays listed as Not set, with Bind and Reset; Reset saves `--reset` [decision 222].
- **Refused when**: an unknown action, or a plugin that is not enabled or has no such command — `invalid` [op/].

## Preference

Exactly these keys exist: `confirm-archive` [person 2026-10-08], `agent-integration`, `notify` [decision 212], `plugin-status` [decision 157], `update-check` [decision 113], `setup-guide` [decision 218], `sidebar-order`, `sidebar-hide-empty-projects`, `sidebar-unread-first` [decision 232], `terminal-font-size`, `branch-prefix`, `density`, `font-ui`, `font-terminal`, `terminal-copy-on-select`, `terminal-links-browser`, `review-viewed`, `slot.threads` and `slot.review` (a plugin id, or `kagan` for the built-in; set from the window or with `kg plugin use`) [docs:plugin-api], and the layout of each window, `<window>.left-sidebar`, `<window>.left-width`, `<window>.right-sidebar`, `<window>.right-width` and `<window>.review-wrap`, where `<window>` is a lowercase id such as `main` [decision 48]. `row-actions` and `footer` were removed as unused: the window never read them. The bare `right-sidebar` name was removed as unused (decision 48). The dock layout is not a preference and not an `Op`; it is `{dataDir}/layout.json` [decision 15]. `predict` still loads and no longer hides hunks; it is not one of these keys [decision 43].

### `Get`

- **Shows**: the stored value. An unset key returns null [person 2026-10-08]. The window treats an unset `confirm-archive` as off [person 2026-10-08]. The window treats an unset `terminal-copy-on-select` as on [decision 231].
- **Refused when**: any other key — `invalid`, message `unknown preference <key>` [person 2026-10-08] [op/].

### `Set`

- **Changes**: stores the value when the shape matches [person 2026-10-08]. `confirm-archive` is a boolean [person 2026-10-08], and so are `agent-integration` and `plugin-status` [decision 157]. `notify` is one of `all`, `asks` or `off`, and any other value is refused [decision 212]. `agent-integration` set to `false` drops kagan's tools from claude (--mcp-config, --plugin-dir, --append-system-prompt-file, --disallowed-tools, and --session-id or --resume, so Start again does not resume the conversation), codex (--disable multi_agent, --disable multi_agent_v2, -c mcp_servers.kagan.*, and resume <id>), cursor-agent and agent (--plugin-dir, the workspace rule, and --new-session-id or --resume), and pi (--skill and --append-system-prompt). Every program still gets the KG_ variables and PATH with kg's folder first, and the environment variables set under `## Env` [decision 220]. `notify` sets which events post a system notification for a thread that is not on screen: `all` (a turn ended, a question or approval waits, or a program failed or exited), `asks` (only a question or approval), or `off` [decision 212]. An unset `notify` counts as `all`. A thread's own level replaces it [decision 213]. `plugin-status` set to `false` hides every plugin item in the activity strip; an unset one counts as on, and a plugin can still be turned off in Installed [decision 157]. `setup-guide` is a boolean; `false` hides the setup guide on the empty window, and an unset one counts as shown [decision 218]. `update-check` is a boolean; an unset one counts as off, and on runs the automatic update check at launch and then daily [decision 113]. `terminal-font-size` is an integer from 9 to 24. `branch-prefix` is a string that is empty, or up to 40 bytes ending in `/` or `-` that is a valid git branch start and does not begin with `refs/`, such as `bb/`; kagan puts it in front of a branch name it makes, not in front of one you type [decision 223]. `terminal-copy-on-select` is a boolean; an unset one counts as on, and `false` leaves the clipboard alone [decision 231]. `terminal-links-browser` is a boolean and an unset one counts as off [decision 224]. `density` is `compact`, `default` or `roomy`; any other value is refused [decision 225]. `sidebar-order` is `current` or `attention`; any other value is refused [decision 232]. `sidebar-hide-empty-projects` and `sidebar-unread-first` are booleans [decision 232]. `font-ui` and `font-terminal` are family names of up to 80 bytes, or empty for the theme's own font; a name the system does not have falls back to Inter and JetBrains Mono when drawn [decision 229]. `<window>.right-sidebar` is a boolean that says whether the right sidebar is open; an open window reads it when it opens, so a change applies the next time the window opens [decision 48]. `<window>.review-wrap` is a boolean that wraps long lines in the review. `<window>.left-width` and `<window>.right-width` are numbers of pixels. `review-viewed` is a map from a reviewed file's path to the revision marked as viewed.
- **Refused when**: any other key — `invalid`, message `unknown preference <key>` [person 2026-10-08] [op/]. A wrong shape — `invalid` [person 2026-10-08] [op/].

The sidebar list reads `sidebar-order`, `sidebar-hide-empty-projects` and `sidebar-unread-first` [decision 232]. Unset, `sidebar-order` is `current` (newest thread first) and the two flags are off. `attention` orders by attention, newest first. Pinned threads stay first either way. Unread first places unread threads after the pinned ones and before the rest. A read parent keeps its place, so an unread child under it stays under it. A project with no visible thread is left out when hide empty projects is on, except the chosen project and the project whose page is open. Archived and hidden threads stay out of the list [decision 41] [CONTEXT].

## Agent rules

The agent rules are text a thread's program is told when it starts: the global file `<dataDir>/instructions.md`, and a project's `<project>/.kagan/instructions.md` [decision 211]. Each file holds at most 16 KiB (16,384 bytes) [decision 211]. They reach `claude`, `codex`, `cursor-agent` and `pi`, not `kimi` or `opencode` [decision 211] [docs:self-extend].

### `Get`

- **Shows**: the file's text as it is on disk, at most 1 MiB. A missing file shows nothing [decision 211].
- **Refused when**: an unknown project — `not-found` [op/]. A file that cannot be read, or is not UTF-8 — `invalid` [decision 211]. Only a missing file is empty, so a failed read never looks like an empty rules file.

### `Set`

- **Changes**: writes the file and creates its folder. The text comes from the argument, or from `--file` (`-` for stdin) [book:cli].
- **Refused when**: more than 16 KiB — `invalid`, and nothing is written [decision 211]. No text — `invalid`. An unknown project — `not-found` [op/]. A project whose folder is gone — `invalid`, and no folder is made [decision 211].
- **Writes**: through a temp file renamed into place, so a start never reads half a file [decision 211].
- **Applies**: a start reads the files then. A running thread keeps the text it started with. Codex's `kg mcp` server reads them when it lists tools [decision 211].

### `Clear`

- **Changes**: deletes the file. Clearing a missing file succeeds.
- **Refused when**: an unknown project — `not-found` [op/].

## Config

Plugin settings, reached as `kg plugin config <id> get|set|clear` [docs:plugin-api] [book:cli]. Setting types are string, boolean, number, select, folder, project, secret, and provider [docs:plugin-api]. A number is a finite JSON number. A provider setting is left out of the settings read. A thread a plugin starts names its program in `spawn` [docs:plugin-api].

### `Get`

- **Shows**: the setting value [book:cli]. A provider setting is left out of the plugin's settings read [docs:plugin-api]. A secret setting shows `"set"` or `null`, never its text, to every caller [decision 104]. A project setting shows the stored id even when that project was later removed [decision 135].
- **Refused when**: a key the manifest does not declare — `invalid` [docs:plugin-api] [op/].

### `Set`

- **Allowed when**: the key is one the manifest declares [docs:plugin-api]. A folder value is approved by a person on the calling thread [book:cli] [docs:plugin-api]. A project value names a live project [decision 135].
- **Changes**: stores the value [book:cli]. A folder setting is granted read-only [docs:plugin-api]. `kg` reads a value left off the command line from a prompt that does not show it, or from stdin, as text [book:cli]. A secret goes to the system keychain, or to a file only the person can read with a warning, never the database [decision 104].
- **Refused when**: the person declines a folder — `failed` [book:plugins] [op/]. A value of the wrong type — `invalid` [op/]. No value — `invalid` [op/]. A secret that is empty, more than one line, or over 1024 bytes — `invalid`, and the message does not repeat it [decision 104]. A project id that is not a live project — `invalid` [decision 135].

### `Clear`

- **Allowed when**: the key is a secret setting the manifest declares [decision 104], or a project setting [decision 135].
- **Changes**: forgets the secret, or unsets the project; the plugin's settings read shows `null` [decision 104] [decision 135].
- **Refused when**: any other setting — `invalid` [op/].

## Window

The macOS menu bar. These shortcuts are not keymap overrides. Quit asks when a program is running [decision 66].

### `About`

- **Allowed when**: always. The kagan mark's menu on the rail runs it, and the same menu runs `SettingsOp::Open` for Settings… [decision 108].
- **Shows**: its own window with the app icon, kagan, the version with the short commit of the build, a tagline, the GitHub, Book, License and Acknowledgements links, and the copyright. A second About focuses the open window. Escape, Cmd+W, or a click in another window closes it [decision 99] [docs:look].

### `NotifyOp::Test`

- **Allowed when**: always [book:cli].
- **Changes**: posts one test system notification, even when `notify` is `off` [decision 215].

### `SettingsOp::Open` and `SettingsOp::Plugins`

- **Allowed when**: always.
- **Changes**: `Open` shows Settings with the section it last showed, General on the first open. `Plugins` shows Settings with Installed selected, and Installed is where plugins are managed [decision 207]. Neither takes a section name; search finds any row [decision 207].
- **Shows**: the settings list on the left and the chosen section on the right [decision 207].

### `SettingsOp::Project`

- **Allowed when**: the id is a live project [op/] [decision 221].
- **Changes**: shows that project's page, with its environment variables, its default program, its agent rules, and its automations, opening the window if it is closed [decision 221] [decision 236]. Escape or Close leaves the page [decision 221]. The sidebar's "Project settings…" menu item sends this op. The automations list shows each one and the thread each run started, and can add, pause, resume, and delete [decision 236].
- **Refused when**: a missing id — `not-found` [op/].

### `Hide`, `HideOthers`, `ShowAll`

- **Changes**: hides kagan, hides the other applications, or shows them [docs:look].

### `Quit`

- **Changes**: quits at once when nothing is running. When a program is running, the menu, Cmd+Q, and the Dock ask once. Quit ends every running program. Cancel leaves them. There is no setting to skip the question. `kg window quit` does not ask [decision 66]. The question says the programs will stop and resume when kagan opens again [decision 175]. The next open starts a thread again when it was still running [decision 175]. A thread left stopping, because the person had asked it to stop, is marked failed with "kagan quit while the program was running" [decision 46] [decision 63].
- **Shows**: the app exits [docs:look].

### `Minimize`, `Zoom`

- **Changes**: minimizes or zooms the window [docs:look].

### `Close`

- **Changes**: closes the window. Programs keep running [decision 46] [docs:look]. Clicking the Dock icon opens the window again [decision 66].

### `CommandList`, `SearchThreads`, `SavedPrompts`

- **Changes**: opens the window when it is closed, and shows the command list, the list on thread search, or the list of saved prompts. When the list is open, each switches it to its own view [decision 107] [decision 234].
- **Shows**: every window command and each enabled plugin's commands, grouped, with the key that runs it from where the keyboard was. A command that cannot run there is greyed with its reason. Enter runs it from there; Escape or a click outside closes the list and gives the keyboard back. Thread search shows the same rows as `kg thread search --archived`, under Threads and Archived; Enter opens the thread at the matched line [decision 107]. Saved prompts lists the same rows as `SavedPromptOp::Search`, starred first. The field searches title and body. Enter sends the selected body into the shown thread and closes the list, or stays open and shows the refusal. Star and Delete change that row. New saved prompt writes a title and a body in the same list [decision 231].

## Activity strip

The strip is the host's. Its one `Op` is `WindowOp::ToggleActivityLog` [decision 196]; the rest the window reads from state it already has [decision 148]. A plugin adds one approved status item to it, which may be a button that runs one of its declared commands [decision 157] [decision 197].

- **Shows**: closed, one 28 px line across the bottom of the window. `1 agent working` or `N agents working` for threads whose program is busy, or `Quiet`; `N needs you` for the threads `NextThreadNeedingYou` can reach, which is every thread other than the one on screen that is unread or asking; `Update available` when a check found a newer kagan that Not now has not put off; `Installing <source>` while a plugin install runs [CONTEXT] [decision 148]. At the right of that summary row, after install, it draws at most four plugin items in plugin-id order, one per plugin whose manifest declares `ui.status` and whose `setStatus` sets a line: the approved title in `muted_foreground`, then the text in the tone's color, both `type_small`, shortened with an ellipsis and bounded to about 200 px. They show only while the `plugin-status` preference is on and the window is at least 720 px wide; a narrower window keeps the host summary and hides them [decision 157]. The needs-you count, `Update available`, `Installing <source>` and a plugin item that names a command are buttons; the summary text is not a control, and a click elsewhere on the strip does nothing. The log toggle is the `Op` `WindowOp::ToggleActivityLog`, bound to Cmd+Shift+E, which shows or hides the log [decision 196]. It draws only what the workspace, the attention list, the updater, the plugin host and the plugin strip entity already report.
- **Changes**: `ToggleActivityLog`, from its button or Cmd+Shift+E, opens the last seven rows of an in-memory log, newest last: a turn started or ended, a thread other than the one on screen that needs the person, an update found, downloading or ready, a plugin install starting, and a plugin command the window ran. The same key, or the button again, closes it, and Escape closes it while focus is in the strip. The needs-you button runs `NextThreadNeedingYou`; the shown thread is not counted in either place, because that action skips it. `Update available` runs the update notice's main action for this install, and `Installing <source>` opens Settings → Installed. Each of these, and the log button, is its own Tab stop. A status item with a command is a button that runs that command; one without takes no click or focus [decision 196] [decision 197]. It keeps 100 rows in memory and saves none. It never takes the keyboard on its own [decision 148].
- **Refused when**: nothing. A panel placed `bottom` stays refused [decision 56] [decision 59], and a plugin gets one text item, not a drawn strip [decision 157].

## Update

Finds a newer kagan on the GitHub releases page (decision 113). Installing is not an `Op`: only the person installs, from the window.

### `Check`

- **Allowed when**: always, whatever `update-check` says. The kagan menu's Check for Updates…, the command list, and Settings → Updates run it [decision 113].
- **Changes**: reads `releases/latest/download/update.json` once, and only after `update.json.sig` matches kagan's key [decision 115]. A newer version that is not a pre-release becomes the offered update; an equal, older or pre-release version reads as current [decision 113].
- **Shows**: what `Status` shows after the check [decision 113].
- **Refused when**: the list cannot be read or parsed — `failed`, and the state says why [decision 113].

### `Status`

- **Shows**: the running version, how this copy was installed (`app`, `homebrew`, `installer`, `app-image` or `other`), whether the automatic check is on, and the state: `unchecked`, `checking`, `current`, `available` with the version and its release notes, `downloading`, `ready` (downloaded and checked; with `at_quit` it installs when kagan quits) [decision 115], or `failed` with the reason [decision 113].

## Status, Guide, Open, Sidebar

### `Status`

- **Shows**: the current thread, project, and environment when passed or taken from the environment, plus the `kg` path, the data directory, and warnings such as a plugin that failed to load [book:cli] [docs:self-extend].
- **Refused when**: a thread id that is empty or contains a character other than an ASCII letter, digit, `_`, or `-` — `invalid` [op/]. A missing thread — `not-found` [op/].

### `Guide`

- **Shows**: the concepts, and one chapter when named [book:cli] [op/]. The text ships in the binary [docs:self-extend]. Chapters are threads, projects, environments, plugins, skills, ui, customize, and json [op/] [book:cli]. `customize` names where each look, key, preference, agent rule and skill folder lives; the bundled `kg-customize` skill points to it [decision 210].
- **Refused when**: an unknown chapter — `invalid` [op/].

### `Open`

- **Changes**: asks the app to open a native window in the already running process [book:cli] [docs:architecture]. With no process, `kg` starts one [docs:architecture]. The app subscribes for this before plugins start, and keeps that subscription with no window open [docs:architecture].
- **Shows**: a window [book:start].

### `Sidebar`

- **Shows**: projects, sections, and the threads the sidebar shows [book:cli] [docs:plugin-api]. It omits archived threads [decision 41] [docs:look] and hidden threads [CONTEXT].
- **Refused when**: nothing in the sources.

## Cross-Op rules

- `KG_SOCKET` names the socket. When it is unset and `KG_DATA_DIR` is set, that folder is made absolute and the socket follows it: `<data>/kg.sock` on macOS and Linux, or `/tmp/kagan-<sha256>/kg.sock` when that path does not fit a local socket. The `/tmp` folder must be mode `0700`, owned by this user, and not a symlink, or the connection is refused. On Windows the pipe name is derived from the absolute folder. A file that is not a socket is not removed. It is not the default socket of the app that uses `~/.kagan` [decision 124] [book:data]. `KG_SOCKET` still overrides. With both unset, the socket stays the platform default.
- Only a person answers a pending question. Answering is not an `Op`. `kg ui` cannot answer, and synthetic `kg ui` cannot answer [decision 10]. A plugin cannot answer either. An answer removes the pending row and writes `answered`. Deleting the row is not an answer [docs:plugin-api]. Every answer appends `answered` with the interaction's title, the chosen option, and the field values when the question declared fields. That is the trace of an approval on the thread [docs:architecture] [event.rs].
- Archive, unarchive, and stop never ask [decision 12] [docs:architecture] [docs:plugin-api].
- The database is the truth for thread rows and the event log [decision 5] [docs:architecture].
- Opening the app starts again a thread that was still running [decision 175]. A thread left stopping, with no quit mark to resume it, is marked failed with "kagan quit while the program was running". An archived thread left running is marked failed the same way, and a thread left starting whose folder is not ready is marked failed with "kagan stopped while the thread was starting". Archive and environment-removal deadlines resume [docs:architecture].
- A live thread appends a user message when `Send` submits a line [decision 136]. It does not append a message from the screen [docs:architecture].
- A restart drops standalone approvals, because their responders are gone [decision 38] [docs:architecture]. A question with no window open waits until a person answers; it does not time out [decision 13].
- Stopping a thread that is still starting leaves it idle. Stopping a running thread marks it `stopping`; once the program has ended it records `stopped` and `idle` and goes idle, with no attention. A program that exits 0 records `exited` and `idle`. Any other exit records `exited` and `failed`, leaves the thread in `error`, and writes no `idle`. A turn ends when the program's own hook says so, when its spinner goes plain, when a known agent with no hook shows no change in its visible text for 15 seconds, when a hook-run turn is silent for 120 seconds, or when the program ends [decision 235]. Stopping during an open turn records `turn.ended` with `interrupted` beside `stopped` and `idle` [decision 123].
- A child left running that cannot be started again, and an archived child left running, appends a notice on its sidebar parent. A thread that resumes, and a thread left stopping, stay quiet. The lifecycle owner gets no copy [decision 19] [decision 175].
- A child with a parent, not started with `--quiet`, types one line into the parent's program and presses Enter when its turn ends, it asks the person a question, its program exits, or it fails. A stop the person asked for, and the stop's own end, send nothing. The line waits until the parent is idle, the person has not typed into it for 1.5 seconds after Enter, and 30 seconds after the last key of a line they have not sent, and several waiting lines go as one message. A `wait` or `wait-event` on that child that returns its end drops the line queued for it, so the parent is not told twice; a wait from another thread drops the line too [decision 201]. A parent whose program has ended gets nothing [decision 201].
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
