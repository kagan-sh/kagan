# kagan plugin API

Status: this file describes the plugin API. Decisions 48, 56, and 59 say which panels the window draws. Terms follow [CONTEXT.md](CONTEXT.md).

## Shape

- A plugin is **one piece**: one gpui-shell script owns its UI, agent tools, CLI commands and storage. No server/app split and no realtime channel. One plugin may call another's `exports`, and only those (decision 155).
- The **manifest declares every contribution**. kagan reads it at startup without running plugin code and loads the plugin the first time one of its contributions is used.
- kagan **pulls work into the plugin**, it never calls into it. gpui-shell passes plain data only and HostModule functions may not re-enter the script, so the plugin awaits jobs and replies. A `call` from one plugin is a job for the other: kagan queues it there, and the reply goes back to the caller.
- Everything kagan-specific is a **HostModule** under `kagan/<area>`. The module registry is the grant: a plugin sees only the areas its manifest asked for and the user approved.
- **Native work is kagan's.** gpui-shell's own `process` and large `fetch` are not granted: they cap a program at 30 s with an empty environment and a fetch at 8 MiB. Short programs run in kagan's Rust, behind the `process` area (`process.run`). A `downloads` key or `powers.spawn` is refused: no plugin used them.

## Manifest: `kagan.json`

kagan owns the manifest and builds each plugin's gpui-shell `Policy` from it. The plugin ships no `gpui-shell.json` (gpui-shell rejects unknown fields).

```json
{
  "id": "dev.kagan.vscodium",
  "name": "VSCodium",
  "version": "1.0.0",
  "kagan": ">=0.1.0",
  "entry": "main.js",
  "icon": "icons/vscodium.svg",
  "instructions": "Use editor_open to show a file to the user ...",

  "powers": {
    "run": [{ "program": "gh" }, { "program": "git" }],
    "fetch": [{ "host": "api.github.com", "methods": ["GET"] }],
    "clipboard": { "read": false, "write": true },
    "read": ["~/.claude/projects", "~/.codex/sessions"],
    "areas": ["threads", "database", "process"]
  },

  "settings": {
    "personasDir": { "type": "folder", "title": "Personas folder" },
    "sounds": { "type": "boolean", "title": "Sounds", "default": true },
    "retries": { "type": "number", "title": "Retries", "default": 3 },
    "density": { "type": "select", "title": "Density", "options": ["comfortable", "compact"], "default": "comfortable" },
    "board": { "type": "project", "title": "Board project" },
    "apiKey": { "type": "secret", "title": "API key" }
  },

  "commands": [
    { "id": "open", "title": "VSCodium: open", "key": "cmd-shift-e", "when": "thread" },
    { "id": "handoff", "title": "Hand off", "when": "thread", "header": true },
    { "id": "notes", "title": "Notes", "when": "thread", "menu": true }
  ],
  "tools": [
    { "name": "editor_open", "description": "...", "input": "schemas/editor_open.json", "scope": "all" }
  ],
  "cli": [
    { "name": "vscodium", "commands": [{ "name": "open", "usage": "open <path>[:line]" }] }
  ],
  "events": ["thread.idle", "thread.archived"],
  "schedules": [{ "id": "sync", "every": 15 }],
  "skills": ["skills/vscodium"],
  "themes": [{ "id": "midnight", "file": "themes/midnight.json" }]
}
```

Rules:

- `id` is reverse-DNS and unique. It names the plugin's data folder (gpui-shell `set_bundle_id`), so data survives moves and upgrades.
- `kagan` is the oldest compatible app version. A newer app loads it; an older one refuses with the version it needs.
- `powers` and `folder` settings are exactly what the install screen shows, in plain words ("runs gh", "reads a folder you choose"). Unknown keys invalidate the manifest. A `downloads` key, `powers.spawn`, or `powers.serve` is refused. An update whose powers grow asks again.
- `powers.run` names programs found on the user's `PATH`, for short commands (`process.run`). They start with a fixed environment allowlist: `HOME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `XDG_*`, `SSH_AUTH_SOCK`, `GH_*`, `LANG`, `TERM`, `KG_DATA_DIR`, `KG_SOCKET`, and the login-shell `PATH` kagan already resolves for agents. The two `KG_*` names are only there when kagan itself runs with them. `process.run` resolves the program on that PATH and does not add the directory of the running binary. On Windows, where there is no login shell, the lookup uses the app's own PATH. A grant of `kg` fails with `program kg not found` unless `kg` is already on that PATH. On macOS the app binary is `/Applications/kagan.app/Contents/MacOS/kg`. When `kg` is found, those two variables are how the child reaches this app. `"env": "inherit"` passes the full environment and says so on the install screen.
- A plugin's own `process.env` is empty: kagan turns off gpui-shell's default list of variables, which includes proxy URLs that often carry a password, and no power grants one. Programs a plugin runs get the environment `powers.run` describes.
- `process.dataDir` is that plugin's data folder (`<app data>/plugins/<id>/data`), the same path `${dataDir}` names in file roots (decision 127). It needs no power. Reading or writing still needs the root grant.
- `powers.fetch` and `clipboard` map onto gpui-shell capabilities (`network.http`, `clipboard`) for small requests. A fetch `host` is either a remote hostname (HTTPS on 443) or `http://127.0.0.1:<port>` / `http://localhost:<port>` with an explicit port (HTTP on that port only, including `:80`). At most eight local entries. Bare `127.0.0.1` / `localhost`, bare loopback IP literals (including `[::1]`), `0.0.0.0`, `[::]`, `[::ffff:0:0]`, `localhost.`, `*.localhost`, a missing port, remote `http://`, `https://…` URLs, and any IPv6 form are refused. Local requests bypass HTTP proxies and resolve `localhost` and the request host only to loopback. The install screen lists each remote as "talks to \<host> (methods)" and each local port once as "talks to programs on this computer (port N)". `cx.write_to_clipboard` writes text. `host.writeClipboardPng(pngBase64)` writes a PNG when `clipboard.write` is granted: standard base64, no `data:` prefix, at most 8 MiB decoded, including a PNG of exactly 8 MiB. On Linux it returns an error: png clipboard is not available on this platform. File roots are `${pluginDir}` read, `${dataDir}` read-write, plus each `folder` setting's chosen path, read-only, plus each `powers.read` folder. Key-value `storage` is always granted.
- `powers.read` is a list of folders under the user's home, written `~/...` (`"read": ["~/.claude/projects"]`). An absolute path, `..`, or a path that resolves outside home is refused. The kagan data directory, a folder inside it, a folder that contains it, `~/.ssh`, and `~/.gnupg` are refused, including when `KG_DATA_DIR` points outside home. A missing folder is not an error; it becomes a read root when it exists at load. A path that is a file is refused. The install screen lists each one as "reads ~/.claude/projects". Adding a folder on update asks again. gpui-shell `fs/promises` `readFile` reads a whole file off the UI thread and stops at 64 MiB, with no offset, so these folders are also the roots of `kagan/files`.
- `powers.areas` picks HostModule areas, one switch each, no read/write split. `downloads` is not an area.
- A tool name is global to an agent, not namespaced by plugin: it may be declared by one installed plugin only. When two enabled plugins declare the same name, the agent sees the earlier plugin's tool (plugins in id order), and `kg status` and the plugin's Settings row say which plugin the name also belongs to. Prefix tool names with the plugin's own id to keep them distinct.
- `powers.calls` lists the plugin ids this plugin may `call` through `kagan/host`. The install screen says `asks the <id> plugin to do work for it`, and a new id asks again. A plugin cannot list itself.
- `exports` lists the names other plugins may `call`. Names are nonempty and unique. A name not listed is refused before the callee loads. The callee runs the call with its own powers, never the caller's.
- `instructions` is one plugin-level text. `kg mcp` sends it as the MCP server `instructions`; each tool description must still stand alone, since an agent may ignore it.
- `"instruct": true` asks for a job each time a program starts with agent integration. The job is `{ kind: "instruct", threadId, projectId }`. Reply with a string or `{ text }`, at most 4096 bytes; empty, a crash, or a 2-second timeout adds nothing. Each kept reply is prefixed with `Plugin <id>:` and joined replies (at most 16 KiB total, dropping later whole replies) append to that start's note and MCP instructions (decision 130). The install screen says "adds instructions to a thread's program".
- `schedules` runs a job on a timer, while kagan is open: `{ id, every }` with `every` a whole number of minutes, 1 to 1440, and each `id` unique. Each run loads the plugin if it is not loaded, then sends `{ kind: "schedule", name }`. The run ends when the plugin replies or fails, or after 2 minutes. A loop waits for its last run to end before the next wait, so runs never overlap, and a failure or timeout is written to the plugin log. A timeout also retires that load, so the next run starts a new one. The first run comes one interval after the plugin starts. Nothing runs while kagan is closed, and a missed run is not made up. Disabling, removing or reloading the plugin stops the loops. A reload starts them again from zero. The install screen says "runs on its own every 15 minutes while kagan is open", and an update that adds a schedule or changes its interval asks again (decision 146).
- `skills` lists skill folders inside the plugin. Each one joins `kg skill` and, on every start with agent integration, the composed native skills tree beside kagan's bundled skills. kagan reads the list when the plugin is installed, updated, reloaded, or started, so an edit to `kagan.json` shows after `kg plugin reload`.
- `when` on a command is `always`, `thread` (a thread is on screen) or `project` (a project is on screen; Settings covers the window, so none is). A `key` runs its command only in the context `when` names. `"header": true` draws the command as a small button on the shown thread's title bar, before Stop and the thread menu; it needs `when: "thread"` (decision 129). `"menu": true` adds the command as a row in the shown thread's actions menu (the ellipsis), after Rename and before Save diagnostics, labeled with its title; it needs `when: "thread"` too (decision 141). A command may set both, and `menu` is a placement, not a key: the command still has no key unless it declares one.
- A command's `key` is not bound when the window already takes it, with a built-in shortcut or one the person saved. A key takes it when one key sequence starts with the other, so `secondary-n x` overlaps `secondary-n`. A person binds a command's key in Settings → Keyboard, or with `kg keymap set <plugin-id>:<command> <key>`; `kg keymap remove` leaves it with no key and `--reset` goes back to the manifest's. A saved key gets the same check: it answers `{ ok: false, conflict }` when a window key or another plugin command's key takes it, and the conflict is named as `kg keymap` names it (`NewThread`, `<plugin-id>:<command>`). When two plugin commands want the same key, a key the person saved wins over a manifest's, whatever the plugin order; otherwise the first in plugin and command order keeps it. Settings → Keyboard lists every command of each enabled plugin under the plugin's name, with its key or Not set, and a key that is not bound says what took it on both rows. The plugin's log says it too (`kg plugin logs <id>`). A `key` is optional: a command without one is still found and bound there. The keys are bound again when the window's shortcuts change, so a freed key starts working and a taken one stops.
- A tool's `scope` is `all` (every thread) or `tagged` (only threads carrying this plugin's metadata, as vocodr's master thread). kagan validates every tool call's input against its `input` schema before the job and fails the call with the schema error.
- Each `view` names a View class the entry module exports; each `job` names a handler the plugin dispatches on.
- Each of `themes` is an `id` (lowercase letters, digits and dashes, unique in the plugin) and a `file` inside the plugin folder, in the theme file format of [look.md](look.md). Install, update and `kg plugin check` read every file and refuse a bad one with the theme id and the file in the message. The theme's id is `<plugin id>/<id>` (`dev.kagan.example/midnight`): Settings → Appearance lists it under the plugin's name and `kg theme set` takes it. A theme needs no power. Disabling, updating, reloading or removing the plugin drops or re-reads its themes; the window says once when the theme in use goes away, and uses the default (decision 101).

## Lifecycle

1. **Discover**: read `kagan.json` from each installed plugin folder. Register commands, keys, UI contributions, tools, CLI names, settings and skills. No script runs.
2. **Load** on first use: a panel comes on screen, a command runs, a tool is called, a CLI command runs, a subscribed event fires, a schedule is due, or the plugin declares `ui.status` and kagan starts. Declaring `ui.status` also loads it again after a reload or update, so its line stays. gpui-shell loads the entry's default-export View under the plugin's own `Policy` and HostModule registry, in a hidden window (`show: false`) when none of its UI is on screen.
3. **Serve**: that View's `init` starts the job loop with `cx.spawn`; its UI Views render as usual.
4. **Unload** on disable, uninstall or reload: gpui-shell cancels every task under the plugin's `Policy`; kagan closes the plugin's job queue (its pending `next()` rejects, so it cannot take anyone else's job), fails its open jobs, and kills every process tree it started for the plugin.

A crash is an exception, not a process death: kagan logs it, shows it on the plugin row, and reloads on the next use.

## The job loop

```js
// @ts-check
import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        handle(job).then((value) => reply(job.id, value), (error) => fail(job.id, String(error)));
      }
    });
  }
}
```

Each plugin gets **its own** `kagan/jobs` module and queue through `Policy::with_host_module`; queues are never shared, because gpui-shell does not cancel a HostModule future on unload (spike #16). Jobs may be handled at once and replied to in any order, so one held request never blocks a tool call.

A job is plain data with a `kind`:

| kind | payload | reply |
|---|---|---|
| `tool` | `{ name, input, threadId }` | `{ text, isError? }` |
| `cli` | `{ command, argv, cwd, threadId?, projectId? }` | `{ exitCode, stdout, stderr }` |
| `command` | `{ command, threadId?, projectId? }` | none |
| `event` | `{ type, threadId, projectId, data }` | none |
| `page` | `{ panelId, message }` | none |
| `instruct` | `{ threadId, projectId }` | string or `{ text }`, or nothing |
| `schedule` | `{ name }`, the schedule's `id` | nothing. The reply or failure ends the run. |
| `call` | `{ name, input, caller }` | any JSON value; a thrown error or `fail` rejects the caller's `call` with its message |

Timeouts: tool and CLI 120 s, schedule 120 s, call 30 s, instruct 2 s (empty on timeout), others 10 s; the caller gets a plain error naming the plugin, except instruct, which skips a late or failed reply. A `cli` job's `projectId` is the thread's project, else the project whose folder contains `cwd`.

**The UI thread is shared.** Each plugin has its own QuickJS VM, and every one runs on the UI thread. gpui-shell cuts off a call that runs past its budget: 50 ms for render and layout, 500 ms for an event handler or a job, 5 s for loading. A JS loop therefore stalls the whole app for up to that long per call, and the call fails. So heavy work goes to the `process` area (`process.run`). kagan times every render; a plugin whose renders average over 16 ms, repeatedly, is marked slow on its plugin row with an offer to disable it. Jobs are not timed, so a slow job shows only as a stalled window.

## Records and events

Returned by the `threads` area, from the `domain` types.

- **Thread**: `{ id, projectId, environmentId?, title, status, visibility, parentThreadId?, lifecycleOwnerThreadId?, sectionId?, pinned, lastReadAt?, latestAttentionAt, archivedAt?, createdAt, updatedAt }`. `status` is `pending | idle | starting | active | stopping | error`. Unread is `latestAttentionAt > lastReadAt`. A plugin's thread record omits the program line: no provider or permission mode, and no other field a plugin reads holds them.
- **Project**: `{ id, kind, name, path, gitRemoteUrl?, sort }`. `sort` is the stored rank among standard projects; Personal stays last in `sidebar()` and `projects()`.
- **Environment**: `{ id, projectId, path, status, provider, branch?, baseBranch?, retireAt?, error? }`. `path` is the real (canonical) path. `provider` is the environment provider, `project | worktree | personal`, not the agent. `error` is the reason a removal is refused, and nothing else sets it.
- **Events** (`thread.<type>`, subscribable in the manifest, and the types `waitEvent` takes): `created`, `started`, `exited` (`data: { code?, signal? }`, the exit code, or the signal that ended the program), `stopped` (the person stopped the program), `idle` (the row became idle; a clean exit writes it beside `exited`, and Stop writes it beside `stopped`; a failed exit does not), `waiting` (a reply ended: the title went plain after a spinner), `failed`, `archived`, `unarchived`, `deleted`, `answered` (`data: { title, option }`, the person chose that option), `turn.started` (written when the program's title starts with a spinner), `turn.ended` (`data: { outcome: "completed" | "interrupted" | "failed", error? }`, written when that title has stayed plain, or the program stops or exits during the turn; a confirmed quit does not write it; decision 123), `message` (`data: { role, text, messageId? }`; written when `threads.send` submits a line, role `user`; the screen is not parsed into messages; decision 136), `notice` (`data: { severity: "info" | "warning" | "error", text }`), `tool.started` (`data: { id, name }`, written when kagan hands an agent tool call to a plugin, or serves `skill_load`; `id` is kagan's own; decision 154), `tool.ended` (`data: { id, status: "ok" | "error" }`, written when that call's reply lands, with the same `id`; `error` for a refusal, a schema miss, a plugin error, or a timeout; no input, reply or error text; decision 154). An old log may also hold `tool` (`data: { id, name?, status }`), `restore.failed` (`data: { error }`), `snapshot.failed` (`data: { error }`), `reconnecting` (`data: { text }`), `opened`, `rewound`, `file.changed`, `file.read` and `file.predicted`; `Events` reads them back, but a wait, a manifest `events` entry and `thread.<type>` refuse them.

## HostModule areas

Every function is async unless marked sync. Arguments and results are plain data (gpui-shell `HostValue`). Ids are strings.

### `kagan/jobs` (always granted)

`next()`, `reply(id, value)` sync, `fail(id, message)` sync.

### `kagan/panel` (always granted)

`post(panelId, message)` sends one JSON value to that panel's page. The page hears a `kagan` event whose `detail` is the value. At most 1 MiB of JSON. A panel that is not on screen, or that is not a web panel, rejects the call.

### `kagan/host` (always granted)

- `pluginId` sync, `log(level, message)` sync, `settings()` sync. There is no settings-change export. Call `settings()` when the value is needed. A change does not wake the view. `settings()` is the only place a `secret` setting's text appears: it is the text, or `null` until a person sets it. kagan writes `[secret]` in place of the exact text everywhere it leaves the plugin: the plugin log, a load or view error (`kg plugin list`, `kg status`, the panel's failure), a tool reply or tool error, a toast, and `kg <cli>` output. Only the exact text is found, the longest secret first; a split or encoded copy is not.
- `setStatus(text, options?)` sync sets this plugin's one line in the activity strip. It needs `ui.status` in `kagan.json`; a plugin without it is refused with `plugin <id> has no status item; declare ui.status in kagan.json`. `text` is `null` to hide the line, or one plain line of 1 to 120 characters with no control characters and none of U+061C, U+200B to U+200F, U+2028 to U+202E, U+2060 to U+2064, U+2066 to U+2069 or U+FEFF. An empty or whitespace-only string is refused; use `null` to hide, and calling it with no argument is refused. The secret redaction runs first, and the redacted text is then checked against those same limits. `options` is omitted or `{ "tone": "quiet" | "busy" | "needs" | "failed", "tooltip": "..." }`. `tone` defaults to `quiet`, and `tooltip` is optional: one plain line, checked and redacted like `text`, shown when the line is hovered. Any other key or tone is refused. The same text and tone do not redraw. It needs no power and no area. The host owns the layout, colors, accessibility, lifetime and off switch (decision 157).
- `context()` sync → `{ threadId?, projectId?, focused, theme: "light" | "dark", props?, overlay? }` for the window in front; `onContextChanged()` resolves with the next context (the thread on screen changed, focus moved, `openPanel` delivered props, or the overlay area changed size). `overlay` is `{ width, height }` in pixels, the front window's center that a `ui.overlay` view covers, once the window has drawn one. Panels read `context()` in `init` and loop on `onContextChanged()`. `props` is only the object this plugin was given.
- `call(plugin, name, input?)` → a promise of the export's JSON reply. It rejects with a message that names the plugin when the caller did not list it in `powers.calls`, the plugin is not enabled, `name` is not in its `exports`, the input or reply is over 64 KiB, it does not answer within 30 s, or it unloads first. A failed export rejects with its own message, and secrets in the reply are hidden the way tool replies hide them. A call runs from a job the plugin handles. Made at the top of a script, while the plugin loads, it fails the load with `no kagan app context`, as the other app calls do. A plugin cannot call itself. Calls can loop from A to B and back; there is no depth limit, and the 30 s timeout ends a loop.
- `threadOnScreen(threadId)` sync → whether any kagan window shows that thread.
- `openThread(threadId)`, `openPanel(panelId, props?)` (props reach the panel through `onContextChanged`), `openUrl(url)` (system browser; needs `powers.openUrl`, shown on the install card as "opens links in your browser"). `toast(message, action?)` where `action` is `{ label, command }` and runs one of the plugin's commands. `openPanel(panel, props)` shows one of the plugin's declared panels, placed `left`, `center`, or `right`. A left panel opens its sidebar beside the center when that fits, and over the center when it does not. A right panel opens as a tab in the right sidebar. A center panel covers the terminal without stopping it. The call resolves once the panel is on screen, then delivers `props` to this plugin only. It rejects with `<plugin> has no panel <panel> in this window` for a panel the plugin did not declare, so the plugin can catch it. `kg surface list`, `kg surface open`, and `kg surface close` are the same path. `openThread` and `toast` resolve once the window has them. `addComment({ path, side?, start, end, text })` adds a note to the review on screen, only while this plugin draws it (see Replaceable sidebar and review). `pasteIntoThread(threadId, text)` pastes into that thread's program terminal without pressing Enter (same path as review Send and `kg thread paste`), at most 1 MiB; it rejects an empty paste or a missing thread. `writeClipboardPng(pngBase64)` is sync and writes a PNG when `powers.clipboard.write` is granted.

### `threads`

- Threads: `get(id)`, `list({ projectId?, archived?, limit?, before? })`, `spawn({ projectId, program, title?, environment, visibility?, parentThreadId?, quiet?, lifecycleOwnerThreadId?, metadata? })`, where `quiet: true` keeps the parent from being typed a line when this thread's turn ends, waits, exits or fails [decision 201], `stop(id)`, `output(id)` → `{ output }` (the program's kept scrollback as plain text, newest 256 KiB; decision 128), `archive(id)`, `unarchive(id)`, `remove(id)`, `setPinned(id, pinned)`, `rename(id, title)`, `ask(id, title, options, fields?)`, `markRead(id)`, `metadata(id)` / `setMetadata(id, value)` (this plugin's own slice). `rename` stores the trimmed title once and returns the thread; after that the title is kept, and the call fails with "the title is kept; rename it in the window". A thread spawned with a `title` starts with the title kept. The program and the environment stay. `spawn` starts a thread whose program is `program`. `output` is the same plain text `kg thread screen --scrollback` returns for the program's terminal; a missing terminal is refused. `remove` waits for a person before the thread is deleted. Archive, unarchive and stop do not. `send(id, text)` types that text into any thread's running program and presses Enter, at most 1 MiB. It writes into the program's current input: it can run while busy, append to a draft, or answer the prompt on screen. Without bracketed paste, embedded newlines become carriage returns before the final Enter, so one send can feed a shell more than one line. With bracketed paste, those newlines stay inside the paste. One user `message` does not mean one accepted program turn; what Enter does belongs to the program. Empty text, text that is only control characters (including CR, LF and tab alone), a missing thread, a program that has ended, and a failed write are refused. It does not queue. The typed text is a `message` event with role `user` only after the write succeeds. `pasteIntoThread` still does not press Enter.
- `ask(id, title, options, fields?)` raises a question and returns the interaction id at once. Each option is `{ id, label }`, and there are at least two. A second ask while that question is open returns the same id. Optional `fields` are host-drawn controls: `{ type: "string"|"boolean"|"number"|"select", id, label, … }` (select needs `choices` with unique ids; string may have `placeholder` / `default`; boolean and number may have `default`; select may have `default` as a choice id). A select is a dropdown whose first item is None; None answers `null`. A select with `required: true` has no None item: its answer must be one of its choices, and `null` is refused. A number answer is a whole number (`i64`). Ids are at most 64 characters, labels 128, and string defaults, placeholders and answers 4096. At most 32 fields and 32 choices per select. When that thread's terminal is the center, the host opens a popup over it; Tab and Shift-Tab stay inside the popup. Escape dismisses the popup without answering, restores terminal focus, and leaves the card in Questions, and that dismissal lasts until the question goes away. When the thread is not shown, only Questions lists it. While the popup is drawn, Questions does not draw that card. Over a file review, a page, Archived, Settings, or full screen, no popup is drawn and the card stays in Questions. The person answers in the app, and a plugin cannot answer it. The choice arrives as `answered`, with field values when fields were declared.
- Events: `events(id, { afterSeq?, limit?, order? })`, `waitEvent(id, { type, afterSeq?, timeoutMs })`, `wait(id, { timeoutMs })`. `wait` returns `waiting`, `exited` (the row is idle, including after Stop), `failed`, `not-busy`, or `timeout`.
- Projects: `projects()`, `project(id)`. Environments: `environment(id)`, `restoreEnvironment(threadId)`.
- The window's list: `sidebar()` sync → `{ projects, sections, threads }`, the threads the window lists (not hidden, not archived). Changes: `changes(id, scope)` → `{ files, added, removed, base, repository }` and `diff(id, scope, path)` → one file's hunks and, when it fits, its new side. `scope` is `uncommitted | branch | commit`. Both answer only for a thread in that list, and throw at the call otherwise.
- `environment` for `spawn` is `{ kind: "project-default" } | { kind: "project-on", branch: { kind: "checkout", name } | { kind: "new", base } } | { kind: "reuse", environmentId } | { kind: "worktree", baseBranch?, branch? } | { kind: "personal" }`. `branch` on a worktree names the new worktree branch; omitted, it is taken from the thread title. `project-on` with `new` creates a branch from that local branch and names it the same way. `checkout` switches the project folder to that local branch. A base that is not a local branch is refused.

### `database`

The plugin's own SQLite file in its data folder, opened with `foreign_keys = ON`. `migrate(statements[])` (applied once each, in order), `query(sql, params[])` → rows, `run(sql, params[])` → `{ changes, lastId }`, `transaction(statements[])`.

### `files`

Granted when `powers.read` is not empty. Reads stay inside the plugin's read roots (the plugin folder, its data folder, approved folder settings, and the declared home folders). Both calls run off the UI thread.

- `read(path, { offset, maxBytes })` → `{ text, next, size }`. `offset` and `maxBytes` are whole numbers. `maxBytes` is at most 1 MiB. `text` is the UTF-8 in that byte range, shortened so it does not end mid-character; a range that starts mid-character is an error. `next` is the byte offset after `text`. `size` is the file length.
- `list(dir)` → `{ path, size, modifiedMs }[]` for one directory, skipping an entry the handle cannot open. A listing stops at 10,000 entries or 1 MiB of names. Each read root is opened once when the plugin loads, and reads go through that handle. Replacing the directory with a symlink or junction afterwards cannot redirect a read or a listing. A symlink that leaves a root is refused, including one created after the path was accepted.

### `process`

- `run(program, args[], { cwd?, stdin?, timeoutMs? })` → `{ exitCode, stdout, stderr }` for a `powers.run` program (default timeout 60 s).
- kagan owns each process tree (Job Objects on Windows, process groups elsewhere), resolves `.cmd`/`.exe` by PATHEXT on Windows, and kills every tree on unload.

### `browser`

The window's Browser tab, on macOS, Windows and Linux. Granted by `powers.areas` including `browser`. The install card says "opens and drives the Browser tab for a thread it names". Every call names the thread. The verbs match `kg browser` (decisions 143, 147, and 161): `open(threadId, url)`, `status(threadId)`, `snapshot(threadId)`, `goto(threadId, url)`, `back(threadId)`, `forward(threadId)`, `reload(threadId)`, `click(threadId, ref)`, `fill(threadId, ref, text)`, `type(threadId, text)`, `press(threadId, key)`, `select(threadId, ref, option)`, `scroll(threadId, dy)`, `close(threadId)`. `open` and `status` return `{ threadId, thread, url, title, shown }`. `snapshot` returns `{ url, title, controls, text, truncated }`. A control's `ref` is a whole number from the latest snapshot. `fill` takes the text in the call and has no file argument. There is no `evaluate`, no CDP, and no arbitrary selector. One page per window, owned by one thread, under the same rules as `kg browser`. A plugin does not declare a panel to stand in for this tab. On Linux every call fails with "The browser works on macOS and Windows only for now."

## UI

Plugins get what gpui-kit offers, in the Workbench look ([look.md](look.md)), plus a few things only kagan knows.

Plugins contribute **panels** placed `left`, `center`, or `right` (decision 59). A panel names one body: a View class the entry exports (`"view"`), or an HTML file in the plugin folder (`"web"`). Exactly one of those. A web panel cannot set `replaces`. A panel placed `bottom` is refused. A `left` panel is one icon in the rail, and its body sits below the threads in the list while it is open. A second click on that icon closes it. A `center` panel is a page: one icon in the rail below the tool icons, and the open page fills the center under a host title and Close. Opening that same page again keeps the view. Enter and Space on Close return to the terminal; those keys inside the plugin's own input or button do not. Escape returns unless the focused plugin control handles it first. The thread's terminal stays mounted. A `right` panel is a closable tab in the right sidebar (decision 48). It is not a left-rail icon. Open it with a command, `openPanel` / `kg surface open`, or the right tab bar `+`. A file review and the page do not show together: opening the page closes the review, and opening a review leaves the page. Opening Settings closes the review. Docking and zoom are not in the window. A panel with `"follows": "thread"` tracks the thread on screen through `host.onContextChanged()`.

**Not in this version.** kagan refuses a manifest that declares these, at install and at check: `ui.toolbar`, `ui.status-bar`, `ui.sidebar-footer`, `ui.thread-list`, `ui.file-opener`, and a panel placed `bottom`. `ui.status-bar` is refused with `declare ui.status for one line in the activity strip`. `ui.settings` is kagan's settings page for the plugin, below its declared settings.

**Overlay** (decision 100). `ui.overlay` names one View export that kagan draws, with no background of its own, over a running terminal or a plugin page. It is never drawn over Settings, the start screen, a review or Archived, nor while an approval waits or the shown thread has a question; the sidebars, toasts, menus, popovers and dialogs draw over it. An overlay may only paint: `div`, `h_flex`, `v_flex`, text, `image` and `svg` by relative name from kagan's bundled assets (in every plugin view, a URL, an absolute path or `..` is refused), and paths, with their styles, `transition` and `spring`, and code that reads `kagan/host` and runs timers. A handler of any kind (including `on_mouse_down_out`), a hover or other state style, a cursor, a scroll area, a role or label, any other component (Popover, HoverCard, DropdownMenu, Input, Textarea, a nested view, a list), or any window call (focus, dialogs, sheets, notifications, key bindings, `window.*`, input state) fails the overlay for good: it draws nothing and the plugin log says why, until the plugin loads again. Size it from `context().overlay`, the front window's center, and `onContextChanged()`; `window.viewport_size()` is refused. Let the person hide it with a boolean setting or a command the plugin declares; disabling the plugin always removes it. Reload mounts it again. `examples/plugins/overlay` is a small one.

**Status item** (decision 157). `ui.status` is one object with a required `title` of 1 to 40 characters with no control or bidi/format characters; it is not an array, one item per plugin. `command` is optional: the id of one of this plugin's own commands whose `when` is `always`. A command that is not declared, or that needs a thread or project, is refused at check and install. The title is the label shown before the value and the screen-reader name, and unknown keys such as `view`, `web` or `placement` are refused. The plugin pushes one line with `setStatus`; the host draws it at the right of the activity strip's summary row, after the install line, in plugin-id order, at most four items. It shows only while the `plugin-status` preference is on and the window is at least 720 px wide; a narrower window keeps the host's summary and hides the plugin items. Each item is a host-drawn pair: the title in `muted_foreground` and the text in the tone's color, both `type_small`, shortened with an ellipsis when narrow. With `command`, the item is a button: a click runs that command through the same path as a header-menu command. Without it, the item takes no click or focus. Either way, its `tooltip` shows on hover. It is text only: no plugin View, markup or bottom panel. The line is cleared on unload, disable, remove, a reload, and a crashed load. The install screen says `starts with kagan to show <title> in the activity strip` and, with a command, `clicking that line runs <command title>`; the command's id and title must have no control or bidi/format characters; an update that adds `ui.status`, changes the title, or changes the command asks again.

**Components:** the whole gpui-kit catalog through `gpui_shell::components` (git-only, like gpui-shell): Accordion, Alert, Avatar, Badge, Breadcrumb, Button, Collapsible, Combobox, Command, DataTable, DescriptionList, DropdownButton, DropdownMenu, Editor, Field, Form, GroupBox, HoverCard, Icon, Image, Kbd, Label, Link, List, Menu, MenuBar, Message, MessageScroller, Pagination, Popover, Progress, Questionnaire, Radio, RadioGroup, Rating, Resizable, Scroll, Select, Separator, Settings, ShimmerText, Sidebar, Skeleton, Spinner, StatusBar, Stepper, Tab, TabBar, Table, Tag, Text, Textarea, Toolbar, Tooltip, Tree, and gpui-kit's dialog, sheet and notification overlays. kagan sets gpui-kit's theme to the Workbench palette, so plugins match the app without styling.

**kagan components**, registered through gpui-shell's `ComponentRegistry`:

- `ThreadTitle`, `ThreadStatus`: a thread's title and its status bar.
- `Markdown`: rendered message text.
- `KgButton`: a command button with the calls of `Button`: `label`, `tooltip`, `loading`, `size`, `primary`, `secondary`, `danger`, `success`, `warning`, `outline`, `ghost`, `link`, `compact`, `disabled`, `selected`, `on_click`, and children, which sit inside the button. `kg ui` finds, clicks and reads it by its id, with its role, name, state and focus. Two panels that use the same id are refused by `kg ui click` unless `--within` names one. `kg plugin check` warns on a plain `Button` in any plugin script. `kg plugin new` writes a starter panel that uses `KgButton`.
- `KgInput` and `KgTextarea`, imported from `gpui-component`: a text field. Create the state in `init` from that same module. `InputState.new(placeholder?, initial?)` takes two optional strings. `TextareaState.new(initial?)` takes one optional string, the initial value. `new KgInput(state)` and `new KgTextarea(state)` take that state. `gpui-base` exports the same names with a different constructor, `InputState.new({ placeholder, value })` and `TextareaState.new({ placeholder, value, rows })`. Those calls succeed, and `KgInput` or `KgTextarea` then throws because the state is gpui-base's. `kg plugin check` refuses both the options object on the `gpui-component` constructor and the base state passed to `KgInput` or `KgTextarea`. `aria_label(name)` is the accessible name, and `disabled(flag)` stops edits. With a name, `kg ui` finds the field, `kg ui click <name>` focuses it, and `kg ui type` writes into it. The snapshot states the name, the value, and focus. A masked value is eight dots. Without `aria_label`, the field is not a control. The shell's `Input` and `Textarea` stay as the shell draws them: kagan cannot wrap them (decision 55), so `kg ui` does not see them.
- `WebView`: one page, placed as a panel's body. `new WebView(id)` takes one string id. `url(url)` loads that http or https address. The host must be `127.0.0.1` or `localhost`, or a host in this plugin's `powers.fetch`. Any other address is refused, and so is a navigation that leaves those hosts, with a sentence that says why. `store(true)` keeps cookies and storage under this plugin's data folder (`pages/<id>/`) across opens when the install screen approved "keeps page data on macOS 14 or later"; the default is private and gone when the page closes. kagan only notices `store(true)` chained on `new WebView(...)`, so write it that way to keep the data. On macOS before 14 the page stays private and shows that saving needs macOS 14 or later. On Windows and Linux the store is ignored and the page stays private. Camera and microphone stay off either way. The element fills its parent. A right panel whose body is a `WebView` shows the page edge to edge, and only while that panel is on screen. The page is the same `gpui-webview` view as the Browser tab. It is not the Browser tab and has no address bar. A page that lives in the plugin folder uses a web panel instead.

**Web panel** (decision 133; store in 136). `"web"` is a relative path to one `.html` file inside the plugin, such as `"ui/index.html"`. No absolute path, no `..`, no `\`, and no `__kagan` segment. `kg plugin check` reads that file. Left, center, and right all place it. A right or center web panel fills its frame edge to edge. The plugin's entry still loads, so its job loop can answer the page. There is no server. `"store": true` keeps that page's cookies and storage under the plugin data folder the same way `WebView.store(true)` does, and the install screen says "keeps page data on macOS 14 or later". A view panel cannot set `store`; use `WebView.store(true)` instead.

The page is served from the plugin folder on `kagan-plugin://<plugin-id>/`. That origin is private to the plugin. Reads stay inside the folder: `..` is refused, every symlink is refused, and a file over 8 MiB is refused. The page may load only its own files. Known types are html, css, js, mjs, json, svg, png, jpg, gif, webp, ico, woff, woff2, txt, and wasm. Anything else is `application/octet-stream`.

The page gets no kagan powers. It talks only to its plugin. `kagan.post(message)` sends one JSON value, at most 1 MiB, and the plugin's loop receives a `page` job `{ panelId, message }`. Reply `null`. `import { post } from "kagan/panel"` then `post(panelId, message)` delivers that JSON to the page as a `kagan` event (`event.detail`). A value that is not JSON, or that is over the cap, is refused.

Top-level navigation stays on the plugin's own origin. A host in `powers.fetch` is not a navigation target. The page's content security policy is `default-src 'none'`, scripts and styles from `'self'`, images from `'self'` and `data:`, fonts from `'self'`, and `connect-src` of `'self'` plus `https://` and `http://` for each fetch host that is a plain hostname. Frames, objects, `<base>`, and forms are none. Fetch to a granted host is the page's own fetch, and the other host still applies its own cross-origin rules.

`/__kagan/theme.css` is not a file. kagan serves it from memory and the page links it (the bridge adds the link when the page has none). It sets `--bg`, `--surface`, `--raised`, `--hover`, `--ink`, `--muted`, `--line`, `--accent`, `--needs`, `--failed`, `--added`, `--working`, `--selection`, `--input`, and `color-scheme`, plus the theme's density and type as `--space-xs` to `--space-xxl`, `--radius-xs` to `--radius-lg`, `--text-caption`, `--text-small`, `--text-body`, `--text-title`, `--text-mono`, `--row` and `--control`. Those follow the Workbench theme and update when the theme changes.

The page is private by default. Camera and microphone stay off. New windows and downloads do nothing. With `"store": true`, cookies and storage survive under the plugin data folder; removing the plugin clears them. macOS 14 or later keeps that store; every other system, Windows included, ignores it and the page stays private. The page is shown only while its panel is on screen, and dropped when the panel closes. macOS, Windows and Linux draw it through the same page host as the Browser. `kg ui` sees the panel: role document, name "Web panel", value the panel title, hint `kg ui cannot drive this page`. Driving inside the page is out of scope. `examples/plugins/web-panel` posts a message both ways and paints with the theme variables.

**Drag and drop** (decision 122). A `div`, `h_flex` or `v_flex` in a plugin view takes gpui-kit's drag calls. The payload is text, such as a ticket id or a thread id.

- `on_drag(payload, (event, cx) => …)` makes the element a drag source. `event` is `{ payload, phase }`. `phase` is `started` once the pointer moves a few pixels with the button down, then `ended` after a drop, or `cancelled` after Escape or a release outside every target. A press and release without moving is still a click.
- `on_drag_move((event, cx) => …)` on a target hears `{ payload, side }` when the drag enters it, moves between its top quarter, middle half and bottom quarter (`"top"`, `"middle"`, `"bottom"`), or leaves it. `side` is `null` once the pointer has left, and when the drag ends over it.
- `on_drop((event, cx) => …)` hears `{ payload, side }` when the drag is released over the target.

Only the innermost target under the pointer hears a drag, so a card wins over the column around it. A card that does not want a payload does the column's move in its own `on_drop`. The drag follows the pointer outside its source until the button is released. While it lasts, Escape belongs to the drag: it cancels, and the page under it stays open. Only the plugin that started a drag hears it; a release over another plugin's target cancels. Nothing is drawn under the pointer: paint the source as dragged from `started` until `ended` or `cancelled`, and a target as active while its `side` is not `null`. Scroll, click, text selection and `KgButton` work as before when nothing is dragged. An overlay may not use these calls. `kg ui snapshot` lists every drag source and drop target by its `id(...)`, and `kg ui drag` and `kg ui drop` drive them ([ui-driver.md](ui-driver.md)).

**Not in v1:** pinch. gpui-shell's `on_pinch` reaches a plugin view, but kagan does not test or support it, and `kg ui` cannot drive it. Zoom by modifier-wheel and keys.

Manifest form:

```json
"ui": {
  "panels": [{ "id": "canvas", "title": "Canvas", "view": "CanvasPanel", "placement": "right", "follows": "thread" }],
  "settings": "VocodrSettings",
  "overlay": "Glow"
}
```

A left tool's body sits below the threads in the list column. A center page fills the center. A right panel fills the selected tab in the right sidebar. Questions, Changes and Details are built-in right tabs when their content applies; plugin right panels are closable tabs, and `+` lists closed ones. A surface open writes the sidebar open flags only after the target is on screen. A second open or close while one is waiting for its frame is refused, and the first one finishes. A failed open leaves the flags as they were. Left tools, center pages, and right tabs close through `kg surface close` or their own controls. Reload, disable, and removal move focus to Close, the tool's row, or the terminal when the removed view held it, and leave focus where it was otherwise. The tool row is used only when it is on screen; otherwise focus goes to the terminal, or to Settings when it is open. Choosing a thread, opening or leaving Settings, or a plugin reload, disable, or removal while a request is waiting replaces that request: it fails, and the newer change stays. `kg surface list` reports a surface as visible only when its body is in the drawn frame. A request that is still waiting does not count, and a body with no height does not count.

## Replaceable sidebar and review

Decision 105. A panel may offer to fill a slot: `"replaces": "threads"` on a `left` panel, or `"replaces": "review"` on a `center` panel. One panel per slot; `kg plugin check` refuses any other placement and a second panel for the same slot. Such a panel is not a rail tool or a page.

- **The person chooses.** Settings → Installed → Replace parts list Built-in and each enabled plugin that offers the slot. `kg plugin use threads <plugin>` does the same, and asks a person first when run from a terminal or an agent; `kg plugin use review kagan` goes back to the built-in without asking. `kg preference set slot.*` works only from the window. Installing, updating, enabling or reloading never switches a slot. Settings → Installed says "offers a replaceable sidebar" or "offers a replaceable review".
- **The host keeps the rest.** A replacement draws inside its frame and is clipped to it. A sidebar plugin draws the thread list body only: the rail, New thread, the Archived footer, questions and approvals, and the waiting count stay the host's. A review plugin draws the body under the host's review header; the host's footer counts the notes, discards them, and sends them to the thread's terminal the same way as the built-in review. A replacement may not use Popover, HoverCard, Tooltip, DropdownMenu, DropdownButton, Menu, MenuBar, Select, Combobox, Popup, DatePicker, Dialog, AlertDialog, Sheet, Notification or NativeMenuTrigger: they draw above the window.
- **What a plugin reads.** A sidebar plugin reads `threads.sidebar()` and calls `host.openThread`; its `onContextChanged` also wakes when the list changes. A review plugin gets `{ threadId, scope, path }` as its props, reads `threads.changes` and `threads.diff`, and adds notes with `host.addComment`. A note names a changed file of that review and holds at most 4 KiB. It cannot send or discard notes itself. The `threads` power also lets a plugin start programs in a thread's folder, so a review plugin can still change files that way; the install screen lists that power.
- **Back to built-in.** When the plugin is off or removed, stops offering the slot, fails to start or to load the view, its view throws (a render past 50 ms or a load past 5 s throws), or it uses a refused component, the built-in returns at once, the plugin log says why, and the saved choice becomes `kagan`. Use built-in sidebar (Cmd+Alt+B, Ctrl+Shift+Alt+B elsewhere) and Use built-in review (Cmd+Alt+V) work even while the plugin's view holds the keyboard, and the keyboard returns to the window.

`examples/plugins/sidebar` (a plain thread list) and `examples/plugins/review` (a plain unified diff) are the smallest working ones.

## Settings and the CLI

Setting types: `string`, `boolean`, `number` (a finite JSON number; Settings shows a text field), `select`, `folder` (a user-picked folder, granted read-only), `project` (a project id from a dropdown; decision 135), `secret` (a token such as an API key), `provider` (kept so an old manifest parses; `host.settings()` leaves it out). A thread a plugin starts names its program in `spawn`. Plugin settings are `Op`s like every feature: `kg plugin config <id> get|set|clear <key> [<value>]`. The `plugin-status` preference, a boolean that defaults to on, shows or hides every plugin's activity strip item; Settings → Appearance has the switch, and `kg preference set plugin-status false` does the same.

A `string` setting may add `"multiline": true` for an editor that holds newlines; Save saves it, because Enter inserts a line. A `string` or `number` setting may add `"schema"`, one JSON Schema object (decision 140). kagan compiles the schema when it reads the manifest and refuses the manifest, with the setting key, when the schema is not an object or does not compile. It checks a candidate value in `kg plugin config set` and in Settings before saving, and refuses a `default` that fails the schema; the error names the setting and the first failure in plain words. `secret`, `folder`, `project`, `select` and `boolean` refuse `multiline` and `schema`; a secret stays one line of text. The schema is data: kagan compiles it in process, never runs code from it, and does not fetch a `$ref` over the network.

```json
"settings": {
  "notes": { "type": "string", "title": "Notes", "multiline": true },
  "retries": { "type": "number", "title": "Retries", "default": 3, "schema": { "type": "number", "minimum": 1, "maximum": 5 } }
}
```

A `project` setting (decision 135):

- Declare `{ "type": "project", "title": "Board project" }`. There is no default: unset is `null`.
- Settings shows a dropdown: None, then every project in sidebar order. The trigger shows the project's name, None, or Missing when the stored id is gone, with a note to pick another or None. Long names shorten with a tooltip of the full name.
- `settings()` and `kg plugin config get` return the stored id string, or `null` when unset. A removed project keeps its id until clear or another pick; Settings labels that Missing.
- `set` refuses an id that is not a live project. `clear` unsets it (same as picking None).

A `secret` setting (decision 104):

- Settings shows a masked field with Set, or Replace and Clear once one is set. The field is empty after each save and never shows the saved text. `kg ui` reports it as eight dots, whatever its length, or empty.
- `kg plugin config <id> set <key>` with no value asks for it at a prompt that does not show it, or reads one line from stdin, so it stays out of shell history. `clear <key>` forgets it. The text is one line of at most 1024 bytes, and the key at most 64 bytes.
- `get`, `kg ui`, Settings, thread events and `kg diagnostics export` say only whether one is set: `"set"` or `null`. An agent in a thread cannot read it through `kg`. kagan writes `[secret]` in place of the exact text everywhere it leaves the plugin: the plugin log, a load or view error (`kg plugin list`, `kg status`, the panel's failure), a tool reply or tool error, a toast, and `kg <cli>` output. Only the exact text is found, the longest secret first; a split or encoded copy is not.
- It is kept in the macOS Keychain, Windows Credential Manager, or the Linux Secret Service, opened the first time a secret is read or written. With none of those, it goes to `<data>/secrets/<plugin id>.json`, readable only by the person, and Settings and `kg` say so. That file also names, without their text, the keys the keychain holds. It is never in the database. Removing the plugin forgets its secrets, even when its folder is gone; a key the keychain refuses to delete is named in a warning, and the plugin is removed anyway. An update that drops a secret setting forgets that secret.

## Packaging and install

- **Plain JavaScript, no build step.** Files start with `// @ts-check`; kagan generates `kagan.d.ts` beside gpui-shell's `gpui-kit.d.ts`. A plugin's CI may run `tsc --noEmit --checkJs`. What is in git is what runs.
- **Install from git**: `kg plugin install git:<url>@<ref> --plugin <name>`, or `kg plugin install <path>` for a local folder.
- **Repo index** `.kagan/plugins.json` lists the plugins in a repository. Tag releases per plugin: `canvas/v1.2.3`.
- **Plugin index** `.kagan/index.json` lists plugins by name for a team (decision 103): `{ "schemaVersion": 1, "name": "acme", "plugins": [{ "name": "canvas", "description": "Draw beside a thread", "source": "git:https://github.com/acme/plugins@canvas/v1.2.3", "plugin": "canvas" }] }`. `source` is what `kg plugin install` takes, and `plugin` its `--plugin`. An index in git lists only `git:` sources; a local index may list paths inside its folder. `kg plugin index add <git:<url>@<ref> | path>`, `list` and `remove` keep the order; the first index is searched first. `kg plugin search [<text>]` lists matches by name or description, and an index that cannot be read is a warning. `kg plugin install <name>` or `<index>/<name>` installs the first listing with that name through the same approval; the record keeps the git source and commit, not the name. Settings → Indexes does the same. `kg plugin share [<path>]` checks a committed, pushed plugin and prints its index entry, pinned to that commit, and a `kg plugin install git:<url>@<commit>` line.
- **kagan.sh/plugins** lists every GitHub repository with the topic `kagan-plugin`. Listings are not reviewed, and an install from one still shows its powers through the same approval.
- **The install card** says where the code comes from: `From <url> at <ref>, commit <full commit>`, without a password, and the index that listed it with its address. A folder, or a repository whose host and owner match no index the person added, adds a `Caution:` line. Index and plugin names are lowercase ASCII (`a-z 0-9 - _ .`, up to 64), and a description is one line of up to 300 characters. Then the powers in plain words. An update card has the same lines.
- **Locked to a commit.** Install records the resolved commit; the same code runs until `kg plugin update <name>`. Update shows the change, including any new powers, and asks before applying. The commit is the checksum; no signing in v1.
- **Plugin remove and thread delete** from the socket wait for a person. Nothing is deleted until they approve. `ThreadOp::Delete` names the approving thread `approvalThreadId`; `PluginOp::Remove` names it `threadId`. Declining ends the command. Stopping the command takes the question away, and nothing is deleted (decision 93). A click in the window raises that standalone question. Archive, unarchive and stop do not. A plugin cannot answer any question.
- Library dependencies use gpui-shell's own git dependencies, pinned by tag or commit.

## Not in this API

- HTTP routes, background services that stay running, cron expressions, provider and environment-provider contributions, AI services, and hooks. Add one when a plugin needs it. Timers are `schedules` (decision 146).
- Global (app not in front) hotkeys: after v1. kagan's own Browser tab shows a page on macOS, Windows and Linux (decisions 117, 131 and 164). A plugin with the `browser` area drives that tab through `kagan/browser`, with the same verbs as `kg browser` and no second panel (decision 161). `openUrl` still opens the system browser. A plugin shows one page with `WebView` (121), from localhost or a `powers.fetch` host, or its own HTML as a web panel (133). Neither is a general browser, and neither is the Browser tab.

## Measured by the spike

An earlier spike measured a job round trip at p50 16 µs, each plugin after the first at +3.2 MiB and 15 ms to load (runtime once: +30 MiB), and JS-described drawing at about 3.5 µs per path segment; a plugin that draws large scenes should keep its paths few.
