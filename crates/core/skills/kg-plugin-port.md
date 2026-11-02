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

5. **Port the code** into that one script. The default export is a View whose `init` runs the job loop. Other views are named exports. Start files with `// @ts-check`. Use `KgButton` from `gpui-component`, not `Button`. Give every control a stable id.

6. **Check.** `kg plugin check <path>` reads the manifest and the entry the same way install does. Fix every error. A warning about `Button` means switch that control to `KgButton`. Run `kg plugin types <path>` again after you call something new.

7. **Install and try it.** `kg plugin install <path>` waits until the person approves the powers. Do not approve it yourself. Then `kg plugin dev <path>`, exercise the command or the panel, and read `kg plugin logs <id>`. A trial that must not touch the person's app sets `KG_DATA_DIR` and `KG_SOCKET` to a new temp directory first, and stops the headless process it started when the trial is done.

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
| `bb.description` | no description field | close. Unknown keys are refused. The window shows `name`. Put agent-facing text in `instructions`. |
| `engines.bb` | `kagan` (`">=0.1.0"`) | close. A newer app loads it. An older one refuses. |
| `bb.server` factory | default-export View and the job loop | close. kagan pulls jobs. There is no factory and no server process. |
| `bb.app` web entry | View classes in the same script | close. Rewrite the view. Do not ship the web bundle. |
| `bb.host` | none | gap. Cannot move. A second process on another machine has no place. |
| `bb.skills` | `skills`: paths to folders that contain `SKILL.md` | direct. The folder name matches the frontmatter `name`. |
| `bb.themes` CSS file | `themes`: `[{ "id", "file" }]` of a JSON palette | close. See Themes. CSS does not load. |
| `branding.icon` named glyph | `icon` file | close. Use an SVG file in the plugin. A glyph name has no meaning. |
| `branding.logo`, `experimental_icons` | none | gap. Cannot move. One `icon` file is the mark. |
| `settings` string, boolean, select | same `type`, with `title` | close. No per-setting description, schema, option labels, or multiline flag. A select's `default` must be one of `options`. |
| string with `secret: true` | `{ "type": "secret", "title" }` | close. Read it from `settings()` in `kagan/host`. It is `null` until set. Never print it. |
| number setting | none | gap. Store a string and parse it, or use `select` when the choices are few. Say so. |
| `project` setting | none | gap. With the `threads` area, list `projects()` in the view. |
| `settings.onChange` | `onSettingsChanged()` | direct |
| `experimental_set` | none | gap. The person uses Settings or `kg plugin config <id> set`. A secret is set with no value on the command line. |
| `storage.kv` | `localStorage`, then `flush()` | close. Values are strings (`JSON.stringify`). One value is at most 1 MiB, the file at most 8 MiB, and there are at most 4,096 keys. Always granted. |
| `storage.database()` and `migrate` | `kagan/database` | close. Needs `powers.areas` `database`. `migrate(statements)` is append-only. `query` is read-only. `run` and `transaction` write. Attaching another file is refused. |
| `bb.log` | `log` from `kagan/host` | direct. Read it with `kg plugin logs <id>`. |
| `bb.cli.register` one command | `cli`: `[{ "name", "commands": [{ "name", "usage"? }] }]` | close. Declare every subcommand. Parse `argv` yourself. There is no generated help. A `name` that is already a `kg` command never reaches the plugin. Two plugins that declare the same name and subcommand fail at the call. |
| `agents.registerTool` | `tools`: `{ "name", "description", "input", "scope" }` | close. `input` is a JSON Schema file inside the plugin. `scope` is `all` or `tagged` (threads that carry this plugin's metadata). A duplicated name is kept by the plugin id that sorts first. No row label, icon, or tint. |
| `contributeInstructions`, `configure` | `instructions`, one string | close for standing text. Text that changes per thread or provider cannot move. |
| `ui.requestInput` | `threads.ask` or a panel | close for a choice of two or more options. A custom form cannot move. |
| `thread.created`, `idle`, `failed`, `archived`, `unarchived`, `deleted` | `events`: `"thread.<type>"` | close. The job is `{ type, threadId, projectId, data }`, not a thread record. Read `docs/plugin-api.md` for `data`. |
| `thread.active` | `thread.started` | close. The program has started. Confirm the moment against the thread record before you rely on it. |
| `turn.failed` | `thread.turn.ended` | close. `data.outcome` is `completed`, `interrupted`, or `failed`. |
| other bb events, and `experimental_hooks` | none | gap. Cannot move. A hook cannot block or change a turn. |
| HTTP routes, `bb.plugin token`, RPC, realtime | none | gap. Cannot move. The script calls its own functions. There is no route and no channel. |
| `background` service or schedule | none | gap. Cannot move. No clock and no process that outlives a job. A command or an event can do the work while the app is open. |
| providers, environment providers, machines, helper AI services | none | gap. Cannot move. |
| thread side panel, `experimental_tab` on a thread | `ui.panels` `"placement": "right"` | needs the unmerged `right-sidebar-plugin-tabs` change. See Right panels. |
| `navPanel` as a tool under the thread list | `"placement": "left"` | direct. One rail icon. The body sits under the threads. A second click closes it. |
| a full page | `"placement": "center"` | direct. The host draws the title and Close. The terminal stays mounted. |
| `"placement": "bottom"`, `ui.toolbar`, `ui.status-bar`, `ui.sidebar-footer`, `ui.file-opener` | refused | gap. Cannot move. Check names the spot. |
| `experimental_threadList` | left panel `"replaces": "threads"` | close. The person picks it. You draw only the list body. Read `threads.sidebar()` and call `openThread`. |
| a review body | center panel `"replaces": "review"` | close. Props are `{ threadId, scope, path }`. Read `changes` and `diff`. Add notes with `host.addComment`. The host sends them. |
| settings section | `ui.settings`, a View export | close. Declared settings still render above it. |
| `experimental_appOverlay` | `ui.overlay` | close. Paint only: boxes, text, images, svgs, paths, styles, and motion. No handler, hover, cursor, scroll, or other component. Size it from `context().overlay`. |
| composer controls, thread-header buttons, content scripts, homepage sections, message directives, code fences, sidebar footer | none | gap. Cannot move. A left tool or a command is the nearest stand-in. Say which you used. |
| `node` filesystem anywhere | `fs/promises` inside granted roots, and `kagan/files` when `powers.read` is set | close for a read inside a root. Writes land only in the plugin data folder. `files.read` returns at most 1 MiB. Deleting or writing anywhere else cannot move unless a program named in `powers.run` does it. |
| `child_process` | `powers.run` and `kagan/process` `run` | close. Name each program. Default timeout is 60 seconds. The environment is the fixed allowlist unless `"env": "inherit"`, which the install screen states. The shell's own process grant is not available. |
| `fetch` | `powers.fetch`: `{ "host", "methods": ["GET" and/or "POST"] }` | close. Global `fetch`. 30 seconds. Body at most 8 MiB. No other method and no plugin HTTP server. |
| clipboard | `powers.clipboard` `{ "read", "write" }` and `cx.read_from_clipboard` / `cx.write_to_clipboard` | direct |
| open a URL | `openUrl` from `kagan/host` | direct. The system browser. |
| toast | `toast(message, action?)` | close. `action` is `{ label, command }` and runs one of this plugin's commands. |
| `bb plugin build` | none | direct in the other direction. What is in the folder is what runs. No build step. |

`thread.<type>` may be any of: `created`, `started`, `exited`, `stopped`, `idle`, `waiting`, `failed`, `archived`, `unarchived`, `deleted`, `answered`, `turn.started`, `turn.ended`, `message`, `tool`, `restore.failed`, `snapshot.failed`, `notice`, `reconnecting`, `file.changed`, `file.read`, `file.predicted`, `opened`, `rewound`. Any other event name is refused.

A panel may set `"follows": "thread"`. Read `context()` in `init` and loop on `onContextChanged()`. `openPanel` and `kg surface open` / `close` / `list` are the same path.

Copy from `examples/plugins`: `panel`, `cli`, `sidebar`, `review`, `overlay`, `theme`.

## Themes

A theme file is JSON. That is the whole style contract for colors, and CSS is not a second one.

The file lists both modes and the tokens in `docs/look.md`. Colors are opaque `#RRGGBB`. Unknown fields are errors. The window paints from those tokens. Syntax colors stay the built-in ones for the active mode. A stylesheet can select, lay out, and set properties this window never reads, so kagan does not load one.

Port a bb theme by copying its palette into `themes/<id>.json` and declaring `"themes": [{ "id": "<id>", "file": "themes/<id>.json" }]`. The id the person sets is `<plugin id>/<id>`. A stylesheet that only recolors the app becomes that file. One that also changes layout or syntax colors is rewritten by hand, or reported as a gap.

Styles inside a plugin screen are View calls (`div`, `v_flex`, gaps) that use the same tokens. Rewrite those rules into the script. There is no stylesheet to point them at.

## Right panels

A bb thread side panel maps to `"placement": "right"`: a closable tab in the right sidebar, not a rail icon. Open it with a command, `openPanel`, `kg surface open`, or the right tab bar `+`. Questions, Changes, and Details stay the built-in tabs.

That placement needs the `right-sidebar-plugin-tabs` change, which is not in this tree. Until `kg plugin check` accepts `"right"`, check and a new install fail with `panel "<id>": placement "right" is not supported. Use "left" or "center".` An already installed plugin that still says `right` still starts, and that panel is skipped. Use `left` or `center` for the port, and mark the row as waiting on that change. Do not claim the right tab works.

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
Right panel: waiting on right-sidebar-plugin-tabs / not needed
Theme: JSON palette / no theme / CSS left unported
kg plugin check:
Tried:
```
