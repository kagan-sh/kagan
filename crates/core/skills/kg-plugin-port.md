---
name: kg-plugin-port
description: Port a bb plugin into one kagan plugin, map every surface, and report what matches, what is close, and what cannot move.
---

Port one bb plugin into one kagan plugin. Keep the behavior the person sees. Drop a surface only when this map says it cannot move, and say so in the report.

kagan runs one script. `kagan.json` declares every contribution. The person approves powers. Read `kg-plugin-authoring` for the kagan side, and `kagan.d.ts` after `kg plugin types`. A call that is not in that file does not exist. Do not invent one.

## Workflow

1. **Read the bb plugin.** Open `package.json` and the files `bb.server`, `bb.app`, and `bb.host` point at. Read each folder under `bb.skills` (default `skills/`). Note `engines.bb`.

2. **Inventory every surface.** Walk the factory and the web entry. Write down each item below, or "none":
   - Manifest: id, name, description, icon, logo, engines, skills, themes
   - CLI commands and how `argv` is parsed
   - Agent tools, instruction text, and any per-thread tool selection
   - Panels, tabs, overlays, settings views, composer controls, and other web slots
   - Settings and whether a value is secret
   - Key-value storage and the plugin database
   - Events, hooks, HTTP routes, RPC, realtime, schedules, and background services
   - Programs, network hosts, clipboard, and files outside the plugin folder
   - A host entry, a provider, or an environment provider

3. **Map each item** with the table below. Mark it direct, close, or a gap. A gap keeps its workaround, or "cannot move".

4. **Scaffold.** `kg plugin new <name>` writes `kagan.json`, `main.js`, `jsconfig.json`, and the type files. `<name>` is lowercase and hyphenated. The id is `dev.kagan.<name>`. Replace the starter. Point `--dir` at the folder you want.

5. **Port the code** into that one script. The default export is a View whose `init` runs the job loop. Other views are named exports. Start files with `// @ts-check`. Use `KgButton` from `gpui-component`, not `Button`. A field `kg ui` types into is `KgInput` or `KgTextarea` with `aria_label`, not the shell's `Input` or `Textarea`. A local page in a panel is `new WebView(id).url(url)`. Give every control a stable id.

6. **Check.** `kg plugin check <path>` reads the manifest and the entry the same way install does. Fix every error. A warning about `Button` means switch that control to `KgButton`. Run `kg plugin types <path>` again after you call something new.

7. **Install and try it.** `kg plugin install <path>` waits until the person approves the powers. Do not approve it yourself. Then `kg plugin dev <path>`. Try each kind you declared: `kg plugin command <id> <cmd>`, `kg <cli> <sub> …`, and a tool from inside a thread. Read `kg plugin logs <id>`. A trial that must not touch the person's app sets `KG_DATA_DIR` to a new temp directory. The socket follows that folder unless `KG_SOCKET` is set. Stop the headless process the trial started when it is done.

8. **Report** the checklist at the end, filled in.

## Job loop

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

| kind | you receive | you reply |
|---|---|---|
| `command` | `{ command, threadId?, projectId? }` | `null` |
| `tool` | `{ name, input, threadId }` | `{ text, isError? }` |
| `cli` | `{ command, argv, cwd, threadId?, projectId? }` | `{ exitCode, stdout, stderr }` |
| `event` | `{ type, threadId, projectId, data }` | nothing |

A tool or CLI job has 120 seconds. A command or event has 10. The script shares the UI thread: keep a render under 50 ms and a job turn under 500 ms. Longer work goes to `kagan/process` `run`.

`job.command` on a CLI job is the subcommand (`kg weather today sf` arrives as `command: "today"`, `argv: ["sf"]`). Declare every subcommand in the manifest or the call never reaches the plugin.

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
| `project` setting | none | gap. With the `threads` area, list `projects()` in the view. |
| `settings.onChange` | `onSettingsChanged()` | direct |
| `experimental_set` | none | gap. The person uses Settings or `kg plugin config <id> set`. A secret is set with no value on the command line. |
| `storage.kv` | `localStorage`, then `flush()` | close. Values are strings (`JSON.stringify`). One value is at most 1 MiB, the file at most 8 MiB, and there are at most 4,096 keys. Always granted. |
| `storage.database()` and `migrate` | `kagan/database` | close. Needs `powers.areas` `database`. `migrate(statements)` is append-only. `query` is read-only. `run` and `transaction` write. Attaching another file is refused. |
| `bb.log` | `log` from `kagan/host` | direct. Read it with `kg plugin logs <id>`. |
| `threads.get`, `list`, `archive`, `unarchive` | the same names on `kagan/threads` | direct. Each call in this group needs `powers.areas` `threads`. |
| `threads.events` | `events` / `waitEvent` | close. `events` reads the log. `waitEvent` waits for one type. |
| `threads.spawn` | `spawn` | close. Pass `projectId`, `program`, and `environment`. |
| `projects.get`, `projects.list` | `project`, `projects` | direct |
| `environments.get` | `environment` | direct |
| `threads.output` | `threads.output(id)` → `{ output }` | close. Needs `powers.areas` `threads`. Plain scrollback text, newest 256 KiB, not the last assistant message. Pull on `turn.ended`; kagan does not push screen updates. |
| `threads.send`, `threads.queuedMessages`, `providers.models` | none | gap. Cannot move. |
| `bb.cli.register` one command | `cli`: `[{ "name", "commands": [{ "name", "usage"? }] }]` | close. Declare every subcommand. Parse `argv` yourself. There is no generated help. A `name` that is already a `kg` command never reaches the plugin. Two plugins that declare the same name and subcommand fail at the call. |
| `app.commands.register`, `commandPaletteAction` | `commands`: `{ "id", "title", "key"?, "when"? }` | close. `when` is `always`, `thread`, or `project`. A `key` runs only in that context. The job kind is `command`. |
| `experimental_threadHeaderAction`, `experimental_threadAction` with `placement: "header"` | `commands` with `"when": "thread"` and a `key` | close. There is no header button. |
| `agents.registerTool` | `tools`: `{ "name", "description", "input", "scope" }` | close. `input` is a JSON Schema file inside the plugin. `scope` is `all` or `tagged` (threads that carry this plugin's metadata). A duplicated name is kept by the plugin id that sorts first. No row label, icon, or tint. |
| `contributeInstructions`, `configure` | `instructions`, one string | close for standing text. Text that changes per thread or provider cannot move. |
| `ui.requestInput` | `threads.ask` or a panel | close for a choice of two or more options. `threads.ask` needs `powers.areas` `threads`. A custom form cannot move. |
| `thread.created`, `idle`, `failed`, `archived`, `unarchived`, `deleted` | `events`: `"thread.<type>"` | close. The job is `{ type, threadId, projectId, data }`, not a thread record. Read `docs/plugin-api.md` for `data`. |
| `thread.active` | `events`: `"thread.turn.started"` | close. It fires when the title becomes a spinner. `thread.turn.ended` fires when the title stays plain (`completed`), when the program stops during that turn (`interrupted`), or when it exits during the turn (`failed`, or `completed` on exit 0). |
| `turn.failed` | `thread.failed`, or `thread.exited` with a non-zero `data.code` | close. |
| other bb events, and `experimental_hooks` | none | gap. Cannot move. A hook cannot block or change a turn. |
| own `bb.rpc.register` and `bb.realtime.publish`, between this plugin's server and its own app | a function call and `cx.notify()` | direct. Both halves are this one script. |
| `bb.sdk.plugins.callRpc`, `bb.http.route`, `bb plugin token` | none | gap. Cannot move. No call into another plugin, no route, and no token. |
| `background` service or schedule | none | gap. Cannot move. No clock and no process that outlives a job. A command or an event can do the work while the app is open. |
| providers, environment providers, machines, helper AI services | none | gap. Cannot move. |
| thread side panel, `experimental_tab` on a thread, `threadPanelAction` | `ui.panels` `"placement": "right"` | needs right panel support. See Right panels. |
| `navPanel` | `"placement": "center"` | close. It owns a page: a rail spot and a header title. The host draws the title and Close. The terminal stays mounted. Lost: `subPath` routing and deep links, `headerContent`, `experimental_useBadge`. Only its `experimental_sidebar`, and `experimental_sidebarNavigation` with `"placement": "list"`, go to `"placement": "left"`: one rail icon, the body under the threads, a second click closes it. |
| `"placement": "bottom"`, `ui.toolbar`, `ui.status-bar`, `ui.sidebar-footer`, `ui.thread-list`, `ui.file-opener` | refused | gap. Cannot move. Check names the spot. |
| `experimental_threadList` | left panel `"replaces": "threads"` | close. The person picks it. You draw only the list body. Read `threads.sidebar()` and call `openThread`. |
| a review body | center panel `"replaces": "review"` | close. Props are `{ threadId, scope, path }`. Read `changes` and `diff`. Add notes with `host.addComment`. The host sends them. |
| settings section | `ui.settings`, a View export | close. Declared settings still render above it. |
| `experimental_appOverlay` | `ui.overlay` | close. Paint only: boxes, text, images, svgs, paths, styles, and motion. No handler, hover, cursor, scroll, or other component. Size it from `context().overlay`. |
| composer controls, content scripts, homepage sections, message directives, code fences, sidebar footer | none | gap. Cannot move. A left tool or a command is the nearest stand-in. Say which you used. |
| `messageAction`, `pendingInteraction`, `bb.ui.registerMentionProvider`, `bb.ui.registerThreadAction`, `experimental_newThreadPanelAction` | none | gap. Cannot move. |
| `node` filesystem anywhere | `fs/promises` inside granted roots, and `kagan/files` when `powers.read` is set | close for a read inside a root. Writes land only in the plugin data folder; `process.dataDir` names that folder. `files.read` returns at most 1 MiB. Deleting or writing anywhere else cannot move unless a program named in `powers.run` does it. |
| `child_process` | `powers.run` and `kagan/process` `run` | close. Name each program. Default timeout is 60 seconds. The environment is the fixed allowlist unless `"env": "inherit"`, which the install screen states. gpui-shell's own `process` is not granted. |
| `fetch` | `powers.fetch`: `{ "host", "methods": ["GET" and/or "POST"] }` | close. Global `fetch`. 30 seconds. Body at most 8 MiB. No other method and no plugin HTTP server. |
| clipboard text | `powers.clipboard` `{ "read", "write" }` and `cx.read_from_clipboard` / `cx.write_to_clipboard` | direct |
| copy as image | `host.writeClipboardPng(base64)` | direct when `powers.clipboard.write` is granted. Standard base64 PNG, at most 8 MiB. Linux returns an error: png clipboard is not available on this platform. |
| an HTML text field a test types into | `KgInput` or `KgTextarea` with `aria_label` | direct. `kg ui click` uses that name and `kg ui type` writes the value. The shell's `Input` and `Textarea` are not seen. |
| an iframe or embedded page | `WebView` | close. `new WebView(id).url(url)`, http or https on `127.0.0.1`, `localhost`, or a `powers.fetch` host. macOS draws the page; other systems show one sentence. A general remote page cannot move. |
| open a URL | `openUrl` from `kagan/host` | direct. The system browser. |
| toast | `toast(message, action?)` | close. `action` is `{ label, command }` and runs one of this plugin's commands. |
| HTML drag and drop (`draggable`, `onDragStart`, `onDragOver`, `onDrop`, `onDragEnd`) | `on_drag(payload, handler)`, `on_drag_move`, `on_drop` on a `div`, `h_flex` or `v_flex` | close. The payload is text, not a `DataTransfer`. `on_drag` hears `phase` `started`, then `ended` or `cancelled` (the `onDragEnd`). A target hears `side`: `top`, `middle` or `bottom` (quarter, half, quarter), or `null` when the pointer leaves. Only the innermost target hears it, so a card that refuses a payload does the column's move in its own `on_drop`. Nothing follows the pointer: paint the dragged and active looks. Files dragged in from outside cannot move. |
| `bb plugin build` | none | No build step. Drop it. |

`thread.<type>` may be any of: `created`, `started`, `exited`, `stopped`, `idle`, `waiting`, `failed`, `archived`, `unarchived`, `deleted`, `answered`, `turn.started`, `turn.ended`, `message`, `tool`, `restore.failed`, `snapshot.failed`, `notice`, `reconnecting`, `file.changed`, `file.read`, `file.predicted`, `opened`, `rewound`. Any other event name is refused. kagan writes `created`, `started`, `exited`, `stopped`, `idle`, `waiting`, `failed`, `archived`, `unarchived`, `deleted`, `answered`, `notice`, `turn.started` and `turn.ended`. The others are accepted but never arrive. A confirmed quit writes `stopped` and `idle` and does not write `turn.ended`. A crash while a turn is open writes `failed` and does not write `turn.ended`. A working mark that is not the first character of the title, including U+23F3 after a plain prefix, does not open a turn.

A panel may set `"follows": "thread"`. Read `context()` in `init` and loop on `onContextChanged()`. `openPanel` and `kg surface open` / `close` / `list` are the same path.

Copy from `examples/plugins`: `panel`, `cli`, `sidebar`, `review`, `overlay`, `theme`.

## Themes

A theme file is JSON. CSS does not load.

The file lists both modes and the tokens in `docs/look.md`. Colors are opaque `#RRGGBB`. `kg theme new <id>` writes one to copy.

Copy the palette into `themes/<id>.json` and declare `"themes": [{ "id": "<id>", "file": "themes/<id>.json" }]`. The id the person sets is `<plugin id>/<id>`. A stylesheet that only recolors the app becomes that file. One that also changes layout or syntax colors is rewritten by hand, or reported as a gap.

Styles inside a plugin screen are View calls (`div`, `v_flex`, gaps) that use the same tokens. Rewrite those rules into the script.

## Right panels

A thread side panel maps to `"placement": "right"`: a closable tab in the right sidebar, not a rail icon. Open it with a command, `openPanel`, `kg surface open`, or the right tab bar `+`. Questions, Changes, and Details stay the built-in tabs.

That placement needs right panel support. Run `kg plugin check` with `"right"`. If it refuses with `panel "<id>": placement "right" is not supported. Use "left" or "center".`, use `left` or `center` and mark the row not supported yet. An already installed plugin that still says `right` still starts, and that panel is skipped. Do not claim the right tab works until check accepts `"right"`.

`"bottom"` stays refused either way.

## Powers to ask for

List only what the port uses. The install screen shows each one in plain words. An update that adds one asks again.

| need | declare |
|---|---|
| read or change threads, projects, environments, questions, or the review | `powers.areas`: `threads` |
| the plugin's own database | `powers.areas`: `database` |
| a program on `PATH` | `powers.run`: `{ "program": "<name>" }`, and `powers.areas`: `process` |
| that program's full environment | `"env": "inherit"` on that program |
| HTTP | `powers.fetch`: host and `GET` and/or `POST` |
| clipboard | `powers.clipboard`: `read` and/or `write` |
| a folder under the home directory | `powers.read`: `["~/path"]`. Not the kagan data directory, `~/.ssh`, or `~/.gnupg`. |
| a folder the person picks | a `folder` setting. It is read-only. |

Refused, with no workaround: `downloads`, `powers.spawn`, `powers.serve`, and an area named `downloads`, `agents`, `notify`, or `hotkeys`.

Key-value `localStorage` needs no power. A theme needs no power. `kagan/jobs` and `kagan/host` are always there.

## Checklist

Fill this in and give it to the person.

```
Plugin:
Source:
kagan id:

Moved as-is:
Close (what differs):
Gaps (workaround, or cannot move):
Powers asked:
Right panel: not supported yet / supported / not needed
Theme: JSON palette / no theme / CSS left unported
kg plugin check:
Tried:
```
