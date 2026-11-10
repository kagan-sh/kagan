# Mapping each surface

## Mapping

| bb | kagan | fit |
|---|---|---|
| `package.json` `name` | `id` | close. You choose a reverse-DNS id (`dev.example.name`). It is not derived from the package name. |
| `bb.name` | `name` | direct |
| `bb.description` | the index entry's `description` | close. Not a manifest field: an unknown key is refused, and `instructions` is sent to the agent. One line, at most 300 characters, on the index entry `kg plugin share` prints. That command fills it from the first line of `instructions`, or from `name`. Write the entry yourself when the blurb must stay out of `instructions`. Otherwise drop it. |
| `engines.bb` | `kagan` (`">=0.1.0"`) | close. A newer app loads it. An older one refuses. |
| `bb.server` factory | default-export View and the job loop | close. kagan pulls jobs. There is no factory and no server process. |
| `bb.app` web entry | View classes in the same script | close. Rewrite the view. Do not ship the web bundle. |
| `bb.host` | none | gap. Cannot move. A second process on another machine has no place. |
| `bb.skills` | `skills`: paths to folders that contain `SKILL.md` | direct. The folder name matches the frontmatter `name`. |
| `bb.themes` CSS file | `themes`: `[{ "id", "file" }]` of a JSON palette | close. See Themes. CSS does not load. |
| `branding.icon` named glyph | `icon` file | close. Use an SVG file in the plugin. A glyph name has no meaning. |
| `branding.logo`, `experimental_icons` | none | gap. Cannot move. One `icon` file is the mark. |
| `bb.settings.define` | `settings` in `kagan.json` | close. Declare each key in the manifest. There is no call that defines one at runtime. The rows below are the types. |
| `settings` string, boolean, number, select | same `type`, with `title` | close. No per-setting description, schema, option labels, or multiline flag. A select's `default` must be one of `options`. A number is a finite JSON number; there is no min, max, or integer-only check. |
| string with `secret: true` | `{ "type": "secret", "title" }` | close. Read it from `settings()` in `kagan/host`. It is `null` until set. Never print it. |
| `project` setting | `{ "type": "project", "title" }` | close. Stores a project id. Settings shows a dropdown (None, then projects). A removed project keeps its id until clear; Settings says Missing. |
| `settings.onChange` | none | gap. `kagan/host` does not export `onSettingsChanged`. Call `settings()` when the value is needed. A change does not wake the view. |
| `experimental_set` | none | gap. The person uses Settings or `kg plugin config <id> set`. A secret is set with no value on the command line. |
| `storage.kv` | `localStorage`, then `flush()` | close. Values are strings (`JSON.stringify`). One value is at most 1 MiB, the file at most 8 MiB, and there are at most 4,096 keys. Always granted. |
| `storage.database()` and `migrate` | `kagan/database` | close. Needs `powers.areas` `database`. `migrate(statements)` is append-only. `query` is read-only. `run` and `transaction` write. Attaching another file is refused. |
| `bb.log` | `log` from `kagan/host` | direct. Read it with `kg plugin logs <id>`. |
| `threads.get`, `list`, `archive`, `unarchive` | the same names on `kagan/threads` | direct. Each call in this group needs `powers.areas` `threads`. |
| `threads.events` | `events` / `waitEvent` | close. `events` reads the log. `waitEvent` waits for one type. |
| `threads.spawn` | `spawn` | close. Pass `projectId`, `program`, and `environment`. |
| `projects.get`, `projects.list` | `project`, `projects` | direct |
| `environments.get` | `environment` | direct |
| `threads.send` | `threads.send(id, text)` | close. Needs `powers.areas` `threads`. Types the text and presses Enter in any running program, at most 1 MiB. Writes into the current input; what Enter does belongs to the program. Writes a `message` with role `user` only after the write succeeds. Does not queue. |
| `threads.output` | `threads.output(id)` → `{ output }` | close. Needs `powers.areas` `threads`. Plain scrollback text, newest 256 KiB, not the last assistant message. Pull on `turn.ended`; kagan does not push screen updates. |
| `threads.queuedMessages`, `providers.models` | none | gap. Cannot move. |
| `bb.cli.register` one command | `cli`: `[{ "name", "commands": [{ "name", "usage"? }] }]` | close. Declare every subcommand. Parse `argv` yourself. There is no generated help. A `name` that is already a `kg` command never reaches the plugin. Two plugins that declare the same name and subcommand fail at the call. |
| `app.commands.register`, `commandPaletteAction` | `commands`: `{ "id", "title", "key"?, "when"?, "header"? }` | close. `when` is `always`, `thread`, or `project`. A `key` runs only in that context. The job kind is `command`. |
| `experimental_threadHeaderAction`, `experimental_threadAction` with `placement: "header"` | `commands` with `"when": "thread"` and `"header": true` | direct. The host draws the title as a button. No custom component and no hide-when-null. |
| `agents.registerTool` | `tools`: `{ "name", "description", "input", "scope" }` | close. `input` is a JSON Schema file inside the plugin. `scope` is `all` or `tagged` (threads that carry this plugin's metadata). A duplicated name is kept by the plugin id that sorts first. No row label, icon, or tint. |
| `contributeInstructions`, `configure` | `instructions`, and `"instruct": true` | close for standing text. `"instruct": true` gets a job at each agent start; reply with text for that start only (decision 130). Tool and skill selection per start cannot move. |
| `ui.requestInput` | `threads.ask` or a panel | close for a choice of two or more options. `threads.ask` needs `powers.areas` `threads`. A custom form cannot move. |
| `thread.created`, `idle`, `failed`, `archived`, `unarchived`, `deleted` | `events`: `"thread.<type>"` | close. The job is `{ type, threadId, projectId, data }`, not a thread record. Read `docs/plugin-api.md` for `data`. |
| `thread.active` | `events`: `"thread.turn.started"` | close. It fires when the title becomes a spinner. `thread.turn.ended` fires when the title stays plain (`completed`), when the program stops during that turn (`interrupted`), or when it exits during the turn (`failed`, or `completed` on exit 0). |
| `turn.failed` | `thread.failed`, or `thread.exited` with a non-zero `data.code` | close. |
| other bb events, and `experimental_hooks` | none | gap. Cannot move. A hook cannot block or change a turn. |
| own `bb.rpc.register` and `bb.realtime.publish`, between this plugin's server and its own app | a function call and `cx.notify()` | direct. Both halves are this one script. |
| `bb.sdk.plugins.callRpc` | `kagan/host` `call(plugin, name, input)`, with `exports` on the callee and `powers.calls` on the caller | close. The caller names each plugin it calls, and the install screen says so. There is no `outputSchema`, the wait is 30 s, and a call made at the top of a script fails the load. |
| `bb.http.route`, `bb plugin token` | none | gap. Cannot move. No route and no token. |
| `background` schedule | `schedules`: `[{ "id", "every" }]` | close. Whole minutes, 1 to 1440, while kagan is open. The job is `schedule`, `{ name }`; the reply ends the run. A missed run is not made up. Cron and a service that keeps running are still gap. |
| `background` service that keeps running | none | gap. Cannot move. No process outlives a job. A schedule or an event can do the work while the app is open. |
| providers, environment providers, machines, helper AI services | none | gap. Cannot move. |
| thread side panel, `experimental_tab` on a thread, `threadPanelAction` | `ui.panels` `"placement": "right"` | needs right panel support. See Right panels. |
| `navPanel` | `"placement": "center"` | close. It owns a page: a rail spot and a header title. The host draws the title and Close. The terminal stays mounted. Lost: `subPath` routing and deep links, `headerContent`, `experimental_useBadge`. Only its `experimental_sidebar`, and `experimental_sidebarNavigation` with `"placement": "list"`, go to `"placement": "left"`: one rail icon, the body under the threads, a second click closes it. |
| `"placement": "bottom"`, `ui.toolbar`, `ui.status-bar`, `ui.sidebar-footer`, `ui.thread-list`, `ui.file-opener` | refused | gap. Cannot move. Check names the spot. `ui.status-bar` points to `ui.status`. |
| a hosted status line, such as a usage figure or a connection state, when it is plain text | `ui.status`: `{ "title": "Usage" }` plus `setStatus(text, { tone, tooltip })` | close for text only. One line of at most 120 characters in the activity strip, with `quiet`, `busy`, `needs` or `failed`. No view, icon or markup. With `"command"` in `ui.status`, a click runs that command. The host owns the layout, width and off switch. |
| `experimental_threadList` | left panel `"replaces": "threads"` | close. The person picks it. You draw only the list body. Read `threads.sidebar()` and call `openThread`. |
| a review body | center panel `"replaces": "review"` | close. Props are `{ threadId, scope, path }`. Read `changes` and `diff`. Add notes with `host.addComment`. The host sends them. |
| settings section | `ui.settings`, a View export | close. Declared settings still render above it. |
| `experimental_appOverlay` | `ui.overlay` | close. Paint only: boxes, text, images, svgs, paths, styles, and motion. No handler, hover, cursor, scroll, or other component. Size it from `context().overlay`. |
| composer controls, content scripts, homepage sections, message directives, code fences, sidebar footer | none | gap. Cannot move. A left tool or a command is the nearest stand-in. Say which you used. |
| `messageAction`, `pendingInteraction`, `bb.ui.registerMentionProvider`, `bb.ui.registerThreadAction`, `experimental_newThreadPanelAction` | none | gap. Cannot move. |
| `node` filesystem anywhere | `fs/promises` inside granted roots, and `kagan/files` when `powers.read` is set | close for a read inside a root. Writes land only in the plugin data folder; `process.dataDir` names that folder. `files.read` returns at most 1 MiB. Deleting or writing anywhere else cannot move unless a program named in `powers.run` does it. |
| `child_process` | `powers.run` and `kagan/process` `run` | close. Name each program. `process.run` finds it on the login-shell PATH (on Windows, the app's own PATH) and does not add the directory of the running binary, so `kg` fails with `program kg not found` unless that PATH already contains `kg`. On macOS that binary is `/Applications/kagan.app/Contents/MacOS/kg`. When `kg` is found, `KG_DATA_DIR` and `KG_SOCKET` are passed only if this app was started with them. Default timeout is 60 seconds. The environment is the fixed allowlist unless `"env": "inherit"`, which the install screen states. gpui-shell's own `process` is not granted. |
| `fetch` | `powers.fetch`: `{ "host", "methods": ["GET" and/or "POST"] }` | close. Global `fetch`. 30 seconds. Body at most 8 MiB. Host is a remote hostname (HTTPS) or `http://127.0.0.1\|localhost:<port>` (HTTP on that port, including `:80`; at most eight). IPv6 is unsupported. No other method, no any-port local grant, and no plugin HTTP server. |
| clipboard text | `powers.clipboard` `{ "read", "write" }` and `cx.read_from_clipboard` / `cx.write_to_clipboard` | direct |
| copy as image | `host.writeClipboardPng(base64)` | direct when `powers.clipboard.write` is granted. Standard base64 PNG, at most 8 MiB. Linux returns an error: png clipboard is not available on this platform. |
| an HTML text field a test types into | `KgInput` or `KgTextarea` from `gpui-component` | direct. In `init`, `InputState.new(placeholder?, initial?)` and `TextareaState.new(initial?)` take optional strings. `gpui-base`'s `InputState.new({ placeholder, value })` and `TextareaState.new({ placeholder, value, rows })` are different values, and `KgInput` / `KgTextarea` reject them. `kg ui click` uses `aria_label` and `kg ui type` writes the value. The shell's `Input` and `Textarea` are not seen. |
| an iframe or embedded page | `WebView` | close. `new WebView(id).url(url)`, http or https on `127.0.0.1`, `localhost`, or a `powers.fetch` host. `store(true)` keeps cookies and storage under the plugin data folder. macOS, Windows and Linux draw the page. A general remote page cannot move. |
| the plugin's own HTML app | web panel | close. `"web": "ui/index.html"` instead of `"view"`. The page loads only its folder on `kagan-plugin://`. `kagan.post(message)` becomes a `page` job; `post` from `kagan/panel` returns JSON as a `kagan` event. `"store": true` keeps cookies and storage under the plugin data folder. Paint with the theme variables on `/__kagan/theme.css`. `kg ui` sees the panel and cannot drive the page. macOS, Windows and Linux draw it. |
| open a URL | `openUrl` from `kagan/host`, with `powers.openUrl` | direct. The system browser. |
| toast | `toast(message, action?)` | close. `action` is `{ label, command }` and runs one of this plugin's commands. |
| HTML drag and drop (`draggable`, `onDragStart`, `onDragOver`, `onDrop`, `onDragEnd`) | `on_drag(payload, handler)`, `on_drag_move`, `on_drop` on a `div`, `h_flex` or `v_flex` | close. The payload is text, not a `DataTransfer`. `on_drag` hears `phase` `started`, then `ended` or `cancelled` (the `onDragEnd`). A target hears `side`: `top`, `middle` or `bottom` (quarter, half, quarter), or `null` when the pointer leaves. Only the innermost target hears it, so a card that refuses a payload does the column's move in its own `on_drop`. Nothing follows the pointer: paint the dragged and active looks. Files dragged in from outside cannot move. |
| `bb plugin build` | none | No build step. Drop it. |
