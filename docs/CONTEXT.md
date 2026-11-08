# kagan

A native desktop workspace where one person runs programs in threads, on one machine.

## Work

**Project**:
A named body of work, usually one repository, with one folder on this machine. Kind is `standard` or `personal`.
_Avoid_: Repo, workspace (as a synonym), project source, host

**Personal project**:
The single built-in project for work tied to no repository.
_Avoid_: Scratch, default project

**Environment**:
The folder a thread works in, plus its state (`creating`, `provisioning`, `ready`, `error`, `destroyed`). Several threads can share one.
_Avoid_: Workspace, sandbox, checkout (as a synonym)

**Managed environment**:
An environment kagan created and removes a short grace time after the last unarchived thread using it is archived, such as a git worktree. While retiring it can still be reused. A personal folder is never removed this way: it stays until Delete removes it.
_Avoid_: Temporary environment

**Unmanaged environment**:
An environment that points at a folder that already existed and is never removed by kagan, such as the project checkout.

**Environment provider**:
A named way to make an environment. The set is project, worktree, and personal.
_Avoid_: Environment type, backend

## Threads

**Thread**:
One program running in one environment, with a status (`pending`, `idle`, `starting`, `active`, `stopping`, `error`) and an append-only event log.
_Avoid_: Session, chat, task

**Turn**:
The stretch a program's title shows a spinner, until that title stays plain, or the program stops or exits while the spinner is up. `turn.started` and `turn.ended` record it.
_Avoid_: Run, step

**Event**:
One immutable record in a thread's log, such as a message, tool call, file change or status change, numbered by sequence.
_Avoid_: Log entry, message (as a synonym)

**Item**:
A unit of agent output inside a turn, such as a message, a tool call or a delegation, built from one or more events.

**Parent thread**:
The thread a thread is nested under in the sidebar. It changes nothing about lifetime.

**Lifecycle owner**:
A thread that another thread lives and dies with: archiving or deleting the owner does the same to its dependents. Set only at creation.
_Avoid_: Parent (when lifetime is meant)

**Hidden thread**:
A thread kept out of the sidebar, for plugin or background work, still reachable by id.
_Avoid_: Background thread, private thread

**Section**:
A user-named group that threads can be filed under in the sidebar.
_Avoid_: Folder, group

**Archive**:
Retiring a thread: its history and its files stay, its run stops after a short undo grace. It leaves the sidebar. Restore it from Archived.
_Avoid_: Close, delete

**Delete**:
Removing a thread and its history for good, together with its lifecycle dependents.

**Pinned thread**:
A thread the user keeps at the top of the sidebar.

**Attention**:
The moment a thread last needed the person (a reply, a question, a failure); a thread is unread when its attention is newer than the time it was last read.
_Avoid_: Notification, badge

**Busy**:
A running thread whose program's title starts with a spinner character, so the program is busy with a reply. It is read from the title, not from hooks. A program that never sets such a title is never busy.
_Avoid_: Working, active, spinning

**Waiting**:
A running thread that is not busy. Going from busy to waiting marks the thread done until a window shows it. Questions and failures keep their own marks.
_Avoid_: Idle (for a running thread)

**Pending interaction**:
A question that waits for a person to answer, raised by an agent or a plugin, such as a permission request or a form.
_Avoid_: Prompt, dialog

## Agents and extensions

**Screen**:
What a thread's terminal shows right now, kept by the host. A client asks for it and draws it; it never reads the program's raw output.
_Avoid_: Frame, buffer, pane

**Extra terminal**:
A login shell a person opens in a thread's folder from the + of the right sidebar, shown as a tab there and numbered in that thread. It is not part of the thread: no events, no marks, and it ends with the thread's run.
_Avoid_: Split, pane, second thread

**Scrollback**:
The lines a program printed above its screen, kept by the host, 10,000 at most. The person scrolls back through them; typing brings the screen back into view.
_Avoid_: History (as a synonym for the screen), log, buffer

**Provider**:
The program line a thread runs, such as `claude` or `codex`.
_Avoid_: Agent (for the program), backend, model

**Plugin**:
A package installed from git that adds UI, commands, or skills. It starts a thread by naming a program. It never sees a model. Its data folder is under the app data as `plugins/<id>/data`; the script reads that path as `process.dataDir`.
_Avoid_: Extension, add-on

**Plugin index**:
A JSON list of plugins a team publishes, by name, with each one's git source. Search reads it; install by name still asks a person.
_Avoid_: Marketplace (the decision-3 goal, not the file), catalog, store, registry

**Schedule**:
A plugin's timer. Kagan runs one of its jobs every so many minutes while kagan is open, loading the plugin for the run if needed. Not a cron expression, and not a process that keeps running.
_Avoid_: cron job, cron, background job, service

**Skill**:
A packaged set of instructions an agent can load for one kind of task.

**BB skill**:
A skill in the person's BB data folder (`$BB_DATA_DIR/skills`, or `~/.bb/skills`), or in a thread folder's `.bb/skills`. Kagan leaves it where it is and copies it into a thread's native skill tree; it is not one of kagan's four `kg skill` sources [decision 158].
_Avoid_: imported skill, external skill

**Agent integration**:
What kagan adds to a thread's program so it knows it runs inside kagan: the `KG_*` variables and `kg` on `PATH` for every program. For `claude` and `codex`, the `kg mcp` server and a fixed note in its instructions. `claude` also gets `--disallowed-tools Task,Agent`. `codex` also gets `--disable multi_agent` and `--disable multi_agent_v2`. For `claude`, the composed skills as a plugin at `<dataDir>/agent/catalogs/<hash>`. For `cursor-agent`, a per-thread plugin dir under `<dataDir>/agent/threads/<id>/cursor` (skills linked from the catalog, a copy of the note as a rule there) and the note at `<thread folder>/.cursor/rules/kagan.mdc`, which names `KG_THREAD_ID` and not the thread id. A later start in that folder rewrites the file. For `codex`, the composed skills linked into the thread folder at `.agents/skills`; for `opencode`, at `.opencode/skills`; for `kimi`, `--skills-dir` for each existing native root and the composed root. For `pi`, `--skill <catalog>/skills` and `--append-system-prompt` of `<dataDir>/agent/threads/<id>/note.txt`. The note says a helper is another kagan thread and names Task and Agent as the wrong tool. `<dataDir>/instructions.md` is appended to the note, capped at 4096 bytes [decision 172]. The composed tree is immutable per content hash and holds kagan's resolved skills (bundled, plugin, user, project), then the person's BB skills from `<BB_DATA_DIR or ~/.bb>/skills` and `<thread folder>/.bb/skills`; a kagan skill wins a name collision with a BB skill. Plugins that declare `instruct` may append text to that note for the current start only. For `claude`, `codex`, `cursor-agent`, and `agent`, what lets Start again resume the thread's conversation. For `claude`, nothing is added to a line that names a subcommand. For `cursor-agent`, `agent`, `kimi`, `opencode`, and `pi`, nothing is added to a line that names a subcommand. Archiving or deleting a thread removes its agent files. The links in the thread folder stay, and each one has its own `.git/info/exclude` line. The Cursor rule path has one too. The person can turn the extra arguments off with the `agent-integration` preference.
_Avoid_: Injection, agent setup, wrapper

**Conversation**:
What an agent remembers of a thread's program: what was said and done. Kagan gives `claude`, `cursor-agent`, and `agent` an id for it at the first start, and finds the id `codex` picked, so Start again resumes it.
_Avoid_: Session, chat

**Browser**:
A tab of the right sidebar that shows one web page. On Linux the page is WebKitGTK and needs an X11 window. A thread's agent can drive it with `kg browser`. A plugin can drive that same page when the Browser area is on its install card. The row has Back, Forward, Reload, and Open in external browser.
_Avoid_: Web view, embedded browser

**Browser session**:
The page a thread drives through `kg browser` or `kagan/browser`: one per window, owned by one thread, ending on Stop agent, `kg browser close`, its tab, or kagan quitting.
_Avoid_: Automation, headless browser

**Preview**:
A tab of the right sidebar that shows one file read-only: UTF-8 text, Markdown, PNG or JPEG, or CSV/TSV. One file at a time. It reads the shown thread's environment folder, or a path the person picked, and never edits, saves or fetches.
_Avoid_: Viewer, file opener, page, browser

**Panel**:
A titled block a plugin declares, or one of Questions, Changes, Details and Preview. Questions lists pending questions. A thread mark that says needs you is attention, not that panel. A plugin panel is a left tool, a center page, or a right sidebar tab. A panel placed on the bottom is refused. The right sidebar is a tab bar: built-in panels, closable plugin tabs, the Browser, extra terminals, Preview, and a + that opens one of them.
_Avoid_: Pane, dock

**Page**:
A plugin panel placed in the center. The host draws its title and Close. The thread's terminal stays, and Close shows it again.
_Avoid_: Screen (that word is the terminal), pane, dock

**Overlay**:
A plugin view drawn over the center that takes no input. The sidebars drawn over the center in a narrow window are the left and right overlay; say plugin overlay when both could be meant.
_Avoid_: Layer, HUD

**Slot**:
A part of the window a plugin panel may replace: `threads`, the body of the thread list, or `review`, the body of the file review. The person picks Built-in or one plugin per slot. The host keeps the rail, the questions, the counts and the review footer.
_Avoid_: Spot (that word is for what kagan refuses), takeover, override

**Command list**:
The window's list of every command, with its key, that runs one by name; it also searches threads by title and kept terminal text.
_Avoid_: Palette (that word is the theme's colors), launcher

**KgButton**:
The command button a plugin script builds. It takes the calls of the shell's `Button`, and `kg ui` can find, click and read it. The shell's `Button` cannot be observed, so `kg plugin check` warns on it.
_Avoid_: Button (in plugin scripts), observed button

**KgInput**:
The single-line text field a plugin script builds from `gpui-component`. `new KgInput(state)` takes an `InputState` created with `InputState.new(placeholder?, initial?)`, two optional strings. `aria_label` is its accessible name. `kg ui` can find it, click it, type into it, and read its name, value and focus. The shell's `Input`, and gpui-base's `InputState`, are not this field.
_Avoid_: Input (when `kg ui` must see the field)

**KgTextarea**:
The multi-line text field a plugin script builds from `gpui-component`. `new KgTextarea(state)` takes a `TextareaState` created with `TextareaState.new(initial?)`, one optional string. Same contract as `KgInput`. gpui-base's `TextareaState` is a different value.
_Avoid_: Textarea (when `kg ui` must see the field)

**WebView**:
The page a plugin panel shows. It loads one http or https URL on `127.0.0.1`, `localhost`, or a host the plugin may fetch, and it fills the panel. `store(true)` keeps cookies and storage under the plugin data folder. The Browser tab is separate.
_Avoid_: address bar, embedded browser

**Web panel**:
A plugin panel whose body is an HTML file in the plugin folder. The page loads only that folder, talks to its plugin script, and uses the Workbench theme. `"store": true` keeps cookies and storage under the plugin data folder. `kg ui` can see the panel and cannot drive the page.
_Avoid_: iframe, embedded browser, address bar

**Export**:
A function name a plugin lists in its `exports`, so another plugin may `call` it. Only listed names are reachable.

**Call**:
One plugin running another plugin's export through `call` in `kagan/host`. The caller lists the target in `powers.calls`, and the callee lists the name in `exports`.

**Agent tool**:
A function a plugin or kagan offers to agents in threads.
_Avoid_: MCP tool (the transport, not the concept)

## Updates

**Update**:
A newer kagan release that a check found. Only the person installs it, from the window.
_Avoid_: Upgrade (except in the `brew upgrade` command), patch

**Install kind**:
How this copy of kagan was installed: `app`, `homebrew`, `installer`, `app-image` or `other`. It decides whether kagan replaces itself or names the command or the release page.
_Avoid_: Channel, distribution

```glossary-ban
Add-on	Plugin
Sandbox	Environment
Scratch	Personal project
Session	Thread
Chat	Thread
Backend	Provider
```

```glossary-exceptions
# term	path_contains	line_contains	reason
Badge	plugin-api.md	Badge	gpui-kit component name
Sandbox	picker.rs	sandbox	Provider permission sandbox wording
Sandbox	tests/app/commands.rs	sandbox	Provider permission test copy
Sandbox	plugins/src/runtime/files.rs	sandbox	tempdir variable in path tests
Sandbox	thread/agent.rs	--sandbox	Cursor Agent's own flag name
Session	thread/conversation	"--	Claude Code's own flag names
Session	*	--session-id	Claude Code's own flag name
Session	*	--session	OpenCode, Pi, and Kimi Code's own flag name
Session	*	--new-session-id	Cursor Agent's own flag name
Session	docs/behaviour.md	browser session	browser session is the CONTEXT term for the page a thread drives
Session	book/src/window.md	browser session	browser session is the CONTEXT term for the page a thread drives
Session	crates/ui/src/kagan/browser/agent	browser session	browser session is the CONTEXT term for the page a thread drives
Session	thread/agent.rs	--session	another program's own flag name
Session	thread/agent.rs	"session"	another program's own subcommand name
Session	book/src/skills.md	session opened	another program's own session
Session	docs/self-extend.md	session of the same program	another program's own session
Chat	thread/agent.rs	create-chat	Cursor Agent's own subcommand
```
