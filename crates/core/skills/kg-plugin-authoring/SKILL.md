---
name: kg-plugin-authoring
description: Build, make, fix, check, install, and test a kagan plugin with a panel, a command, an agent tool, or a skill. Use when the person asks to build, make, or fix a plugin, or to add a panel, command, tool or skill to kagan. The person approves its powers.
---

The running app is the contract. `kg plugin types <path>` rewrites `kagan.d.ts` and `gpui-kit.d.ts` from it. Run that again after upgrading kagan, and before you call a host function or a component you have not used. `kagan.d.ts` lists every `kagan/*` export. `kg plugin check <path>` refuses a script the host throws on when the plugin loads or a panel opens: an import that list does not have, a `gpui-component` state constructor whose arguments miss the component schema, and `KgInput` or `KgTextarea` given the `gpui-base` state of the same name. Check does not run the plugin.

Build a plugin in this order. Each step names the command to run.

1. **Start from the starter.** `kg plugin new <name>` writes a folder that already works: `kagan.json`, `main.js`, `jsconfig.json`, and `kagan.d.ts`. The starter has a left tool `hello` with a `KgButton`, and a command `hello` with no key that writes a line to the plugin log. A fixed key would clash with the next starter, so the person binds one in Settings → Keyboard, or with `kg keymap set <id>:hello <key>`. Add `"header": true` and `"when": "thread"` to draw a command as a button on the thread title. Its id is `dev.kagan.<name>`. `<name>` is lowercase and hyphenated.

2. **Edit the manifest, `kagan.json`.** Required: `id` (reverse-DNS), `name`, `version`, `kagan` (`">=0.1.0"`), and `entry` (`"main.js"`). Optional: `instructions` (one standing text for the agent), `"instruct": true` (a job at each agent start that may append text for that start only), `powers`, `settings`, `commands`, `ui`, `tools`, `cli`, `skills`, `events`, `schedules`, `themes`, `icon`.
   - Contributions kagan draws: `ui.panels` with `"placement": "left"`, `"center"` or `"right"`, `ui.settings` (a View export), `ui.overlay` (a View export drawn over a running terminal or a page; it may only paint boxes, text, images, svgs and paths with styles and motion, so no handler, hover style, cursor, scroll area, other component or window call; size it from `context().overlay` and give the person a boolean setting or command to hide it), `ui.status` (one plain line in the activity strip: `{ "title": "Usage" }` with a title of 1 to 40 characters, then `setStatus(text, { tone })` from `kagan/host`; the text is one line of at most 120 characters, `tone` is `quiet`, `busy`, `needs` or `failed`, and `null` hides it; it is text only, so declare a command for an action), `commands`, `tools`, `cli`, `skills`, `events`. `left` is one tool under the thread list. `center` is a page over the terminal, with the host's title and Close. `right` is a closable tab in the right sidebar; it opens from the tab bar's `+`, a command, or `openPanel`. Clicking an open tool closes it; Close returns from a page to the terminal. A failed open does not write sidebar preferences. A second open while one is waiting for its frame is refused. A thread change, Settings, or a plugin reload, disable, or removal while a request is waiting replaces that request. `kg surface list` counts a surface as visible only when its body is in the drawn frame. Enter and Space close a page only while Close has focus.
   - Refused at install and at check, with the reason in the message: `ui.toolbar`, `ui.status-bar` (it points to `ui.status`), `ui.sidebar-footer`, `ui.thread-list`, and a panel placed `bottom`. Remove the refused spots.
   - Replaceable sidebar and review: a `left` panel with `"replaces": "threads"` draws the thread list body, and a `center` panel with `"replaces": "review"` draws the review body. It shows only after the person picks it in Settings → Appearance, or approves `kg plugin use <threads|review> <id>`. Read `threads.sidebar()` and wake on `onContextChanged`; a review panel gets `{ threadId, scope, path }` as props, reads `threads.changes`/`threads.diff`, and adds notes with `host.addComment`, which kagan's footer sends. The frame clips the view, and a replacement may not use Popover, HoverCard, Tooltip, DropdownMenu, DropdownButton, Menu, MenuBar, Select, Combobox, Popup, DatePicker, Dialog, AlertDialog, Sheet, Notification or NativeMenuTrigger. A note names a changed file and holds at most 4 KiB. A view that throws, or uses a refused component, puts the built-in back and writes why to the plugin log. Start from `examples/plugins/sidebar` or `examples/plugins/review`.
   - `themes` lists theme files inside the plugin: `[{ "id": "midnight", "file": "themes/midnight.json" }]`, in the format of a person's theme file (`kg theme new <id>` writes one to copy from). `kg plugin check` reads each file and names the one that fails. The theme shows in Settings → Appearance under the plugin's name, with the id `<plugin id>/midnight` for `kg theme set`. A theme needs no power.
   - `kg surface list` shows every declared surface and whether it is on screen. `kg surface open <plugin> <id>` and `kg surface close <plugin> <id>` use the same path as `openPanel` and the window menus. Open succeeds only when the target is on screen. A sidebar that would shrink the center is refused, and nothing changes.
   - Setting types are `string`, `boolean`, `number` (a finite JSON number), `select`, `folder`, `project`, `secret`, and `provider`. A number needs no power and shows as a text field in Settings. A setting that holds a token, such as an API key, is `{ "type": "secret", "title": "Linear API key" }`. The person sets it in Settings or with `kg plugin config <id> set <key>` and no value, which asks at a prompt. Read it from `settings()` in `kagan/host`; it is `null` until set. Never show it in a view, a tool reply, or a log line: `kg` itself only ever says `set`, and an agent cannot read it. kagan writes `[secret]` over the exact text in your log, errors, toasts, tool replies and CLI output, but not over a split or encoded copy.
   - A setting that picks a project is `{ "type": "project", "title": "Board project" }`. Settings shows a dropdown (None, then every project). `settings()` returns the project id string, or `null` until set. If that project is later removed, the id stays until clear or another pick; Settings labels it Missing. `kg plugin config clear <key>` unsets it.
   - `powers.areas` grants host areas such as `threads`, `database`, and `process`. Call only the areas you list. An unlisted area fails with an error that names it. A `downloads` key, `powers.spawn`, `powers.serve`, `ui.file-opener`, or an area named `downloads`, `agents`, `notify`, or `hotkeys` is refused.
   - `events` lists `thread.<type>`. `thread.turn.started` arrives when a program's title becomes a spinner. `thread.turn.ended` arrives when that title stays plain (`data.outcome` is `completed`), when the program stops during the turn (`interrupted`), or when it exits during the turn (`failed`, or `completed` on exit 0). `message` arrives when `threads.send` submits a line; `tool.started` and `tool.ended` surround each agent tool call kagan serves, with the tool's name, kagan's own id, and `ok` or `error`.
   - `schedules` runs a `schedule` job every so many whole minutes (1 to 1440) while kagan is open. The job is `{ kind, name }`. Reply when the run is done, or fail. Kagan loads the plugin for each run, and a missed run is not made up.
   - `exports` lists the names other plugins may `call`. `powers.calls` lists the plugin ids this one may call. A `call` arrives as a job `{ name, input, caller }` and replies with any JSON value. The caller's `call` is refused, before the callee loads, when it is not approved for that plugin, the plugin is disabled, or the name is not exported. A call made at the top of a script, while the plugin loads, fails the load.

3. **Write the views and jobs in `main.js`.** The entry's default export is a View class whose `init` starts the job loop. Every other View is an export with the name the manifest gives it:

   ```js
   // @ts-check
   import { View, div } from "gpui-kit";
   import { KgButton } from "gpui-component";
   import { log } from "kagan/host";
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

   export class HelloPanel extends View {
     init() { this.count = 0; }
     render() {
       return div().child(
         new KgButton("count")
           .label(`Count ${this.count}`)
           .on_click((_event, cx) => { this.count++; cx.notify(); }),
       );
     }
   }

   async function handle(job) {
     if (job.kind === "command" && job.command === "hello") {
       log("info", "hello from the starter plugin");
       return null;
     }
     throw new Error(`Unknown job ${job.kind}`);
   }
   ```

   Job kinds the loop receives, and the reply each one expects:
   - `command` `{ command, threadId?, projectId? }`: reply `null`.
   - `tool` `{ name, input, threadId }`: reply `{ text, isError? }`.
   - `event` `{ type, threadId, projectId, data }`: reply nothing.
   - `cli` `{ command, argv, cwd, threadId?, projectId? }`: reply `{ exitCode, stdout, stderr }`.

   Host calls come from `kagan/host` (`log`, `openPanel`, `openThread`, `toast`, `openUrl`), `kagan/threads`, and the other `kagan/*` modules named by `powers.areas`. `threads.send(id, text)` needs the `threads` area. It types the text and presses Enter in any running program, at most 1 MiB, into whatever that program is reading now, and writes a user `message` only after the write succeeds. It does not queue. Empty text, control-only text, and a program that is not running are refused. `process.dataDir` is this plugin's data folder (read-write through `fs/promises`); `process.env` is empty.

   Files under that folder need no extra power. A read outside it needs `powers.read` or a `folder` setting.

   `process.run(program, args)` finds `program` on the login-shell PATH (on Windows, the app's own PATH). It does not add the directory of the running binary. `powers.run` of `kg` fails with `program kg not found` unless that PATH already contains `kg`. On macOS the app binary is `/Applications/kagan.app/Contents/MacOS/kg`, and that directory is not on the login-shell PATH. When `kg` is found, `KG_DATA_DIR` and `KG_SOCKET` are passed only if this app was started with them, and the child then talks to this app.

4. **Use `KgButton` for buttons.** Import it from `gpui-component`. A plain `Button` cannot be found or clicked through `kg ui`, and `kg plugin check` warns about it. `KgButton` takes the same calls as `Button`: `label`, `tooltip`, `primary`, `outline`, `compact`, `disabled`, `selected`, `on_click`, and children. Give every control a stable id. Two panels with the same id need `--within` to pick one. A text field `kg ui` can click and type into comes from `gpui-component`. In `init`, `InputState.new(placeholder?, initial?)` takes two optional strings and `TextareaState.new(initial?)` takes one optional string. `new KgInput(state)` and `new KgTextarea(state)` take that state, and `aria_label` is the name. `gpui-base` exports the same names as `InputState.new({ placeholder, value })` and `TextareaState.new({ placeholder, value, rows })`. Those calls succeed, and `KgInput` or `KgTextarea` then throws. The shell's `Input` and `Textarea` are not seen. A remote page in a panel is `new WebView(id).url(url)`: http or https, host `127.0.0.1`, `localhost`, or a `powers.fetch` host. `store(true)` keeps cookies and storage under the plugin data folder. macOS, Windows and Linux draw it. A page that already lives in the plugin folder is a web panel: `"web": "ui/index.html"` instead of `"view"`, and `"store": true` keeps that page the same way. The page calls `kagan.post(message)` and hears `kagan` events. The script answers with `post(panelId, message)` from `kagan/panel` on a `page` job. `kg ui` sees the panel and cannot drive the page. Use the theme variables from `/__kagan/theme.css`. See `examples/plugins/web-panel`. `writeClipboardPng(base64)` on `kagan/host` writes a PNG when `powers.clipboard.write` is granted, and on Linux it fails because a png clipboard is not available on this platform.

   Drag and drop uses gpui-kit's calls on a `div`, `h_flex` or `v_flex`: `on_drag(payload, handler)` on the source, `on_drag_move` and `on_drop` on each target. The payload is text. A target hears `side` (`"top"`, `"middle"`, `"bottom"`, or `null` when the pointer leaves), and only the innermost target hears a drag. Paint the dragged and active looks yourself from what the handlers hear. Give each source and target an `id(...)`: `kg ui drag <from> <to> --at top` drives it.

5. **Check it.** `kg plugin check <path>` reads the manifest, the entry, and each contribution the same way install does, and reports a refused contribution by name. It also refuses a missing host export and a field constructor the component schema rejects. Fix every error before install. `kg plugin types <path>` writes `kagan.d.ts` for this version.

6. **Install, and wait for the person.** `kg plugin install <path>` (or `git:<url>@<ref>`) blocks until the person approves the plugin's powers in the app. A declined install fails the command. Do not approve it yourself, with `kg ui` or the operating system.

7. **Test the command and the log.** Run `kg plugin command <id> hello`, then `kg plugin logs <id>`. The line `hello from the starter plugin` means the command ran. `kg plugin logs <id> --follow` streams new lines.

8. **Iterate.** `kg plugin dev <path>` watches a local plugin and reloads it on each change. Find and click the panel's controls with `kg ui snapshot` and `kg ui click <id>`. Check the screen with `kg ui screenshot`.

9. **Share it.** Commit and push the plugin, then run `kg plugin share <path>`. It checks the plugin, refuses uncommitted changes or a commit no remote branch holds, and prints an index entry that pins that commit plus a `kg plugin install git:<url>@<commit> --plugin <folder>` line. Paste the entry into `plugins` in the team index's `.kagan/index.json` (top level: `"schemaVersion": 1` and a lowercase `"name"` (`a-z 0-9 - _ .`, up to 64, as are plugin names; a description is one line); unknown keys are refused; an index in git lists only `git:` sources), then commit and push the index. Teammates run `kg plugin install <name>` or `<index>/<name>`, and each one approves the powers. `kg plugin update <id>` follows the ref in the entry: a commit pins it; a branch lets update follow.

Unavailable host areas fail explicitly. Do not invent an API: if a call is not in `kagan.d.ts`, it does not exist in this version.
