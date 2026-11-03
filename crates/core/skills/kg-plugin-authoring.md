---
name: kg-plugin-authoring
description: Build, check, install, and test a kagan plugin with a panel, a command, an agent tool, or a skill. The person approves its powers.
---

Build a plugin in this order. Each step names the command to run.

1. **Start from the starter.** `kg plugin new <name>` writes a folder that already works: `kagan.json`, `main.js`, `jsconfig.json`, and `kagan.d.ts`. The starter has a left tool `hello` with a `KgButton`, and a command `hello` with no key that writes a line to the plugin log. A fixed key would clash with the next starter, so the person binds one in Settings → Keyboard, or with `kg keymap set <id>:hello <key>`. Its id is `dev.kagan.<name>`. `<name>` is lowercase and hyphenated.

2. **Edit the manifest, `kagan.json`.** Required: `id` (reverse-DNS), `name`, `version`, `kagan` (`">=0.1.0"`), and `entry` (`"main.js"`). Optional: `instructions` (one text for the agent), `powers`, `settings`, `commands`, `ui`, `tools`, `cli`, `skills`, `events`, `themes`, `icon`.
   - Contributions kagan draws: `ui.panels` with `"placement": "left"`, `"center"` or `"right"`, `ui.settings` (a View export), `ui.overlay` (a View export drawn over a running terminal or a page; it may only paint boxes, text, images, svgs and paths with styles and motion, so no handler, hover style, cursor, scroll area, other component or window call; size it from `context().overlay` and give the person a boolean setting or command to hide it), `commands`, `tools`, `cli`, `skills`, `events`. `left` is one tool under the thread list. `center` is a page over the terminal, with the host's title and Close. `right` is a closable tab in the right sidebar; it opens from the tab bar's `+`, a command, or `openPanel`. Clicking an open tool closes it; Close returns from a page to the terminal. A failed open does not write sidebar preferences. A second open while one is waiting for its frame is refused. A thread change, Settings, or a plugin reload, disable, or removal while a request is waiting replaces that request. `kg surface list` counts a surface as visible only when its body is in the drawn frame. Enter and Space close a page only while Close has focus.
   - Refused at install and at check, with the reason in the message: `ui.toolbar`, `ui.status-bar`, `ui.sidebar-footer`, `ui.thread-list`, and a panel placed `bottom`. Remove the refused spots.
   - Replaceable sidebar and review: a `left` panel with `"replaces": "threads"` draws the thread list body, and a `center` panel with `"replaces": "review"` draws the review body. It shows only after the person picks it in Settings → Appearance, or approves `kg plugin use <threads|review> <id>`. Read `threads.sidebar()` and wake on `onContextChanged`; a review panel gets `{ threadId, scope, path }` as props, reads `threads.changes`/`threads.diff`, and adds notes with `host.addComment`, which kagan's footer sends. The frame clips the view, and a replacement may not use Popover, HoverCard, Tooltip, DropdownMenu, DropdownButton, Menu, MenuBar, Select, Combobox, Popup, DatePicker, Dialog, AlertDialog, Sheet, Notification or NativeMenuTrigger. A note names a changed file and holds at most 4 KiB. A view that throws, or uses a refused component, puts the built-in back and writes why to the plugin log. Start from `examples/plugins/sidebar` or `examples/plugins/review`.
   - `themes` lists theme files inside the plugin: `[{ "id": "midnight", "file": "themes/midnight.json" }]`, in the format of a person's theme file (`kg theme new <id>` writes one to copy from). `kg plugin check` reads each file and names the one that fails. The theme shows in Settings → Appearance under the plugin's name, with the id `<plugin id>/midnight` for `kg theme set`. A theme needs no power.
   - `kg surface list` shows every declared surface and whether it is on screen. `kg surface open <plugin> <id>` and `kg surface close <plugin> <id>` use the same path as `openPanel` and the window menus. Open succeeds only when the target is on screen. A sidebar that would shrink the center is refused, and nothing changes.
   - A setting that holds a token, such as an API key, is `{ "type": "secret", "title": "Linear API key" }`. The person sets it in Settings or with `kg plugin config <id> set <key>` and no value, which asks at a prompt. Read it from `settings()` in `kagan/host`; it is `null` until set. Never show it in a view, a tool reply, or a log line: `kg` itself only ever says `set`, and an agent cannot read it. kagan writes `[secret]` over the exact text in your log, errors, toasts, tool replies and CLI output, but not over a split or encoded copy.
   - `powers.areas` grants host areas such as `threads`, `database`, and `process`. Call only the areas you list. An unlisted area fails with an error that names it. A `downloads` key, `powers.spawn`, `powers.serve`, `ui.file-opener`, or an area named `downloads`, `agents`, `notify`, or `hotkeys` is refused.

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

   Host calls come from `kagan/host` (`log`, `openPanel`, `openThread`, `toast`, `openUrl`), `kagan/threads`, and the other `kagan/*` modules named by `powers.areas`.

4. **Use `KgButton` for buttons.** Import it from `gpui-component`. A plain `Button` cannot be found or clicked through `kg ui`, and `kg plugin check` warns about it. `KgButton` takes the same calls as `Button`: `label`, `tooltip`, `primary`, `outline`, `compact`, `disabled`, `selected`, `on_click`, and children. Give every control a stable id. Two panels with the same id need `--within` to pick one. A text field `kg ui` can click and type into is `KgInput` (`InputState`) or `KgTextarea` (`TextareaState`), with `aria_label` as its name. The shell's `Input` and `Textarea` are not seen. A remote page in a panel is `new WebView(id).url(url)`: http or https, host `127.0.0.1`, `localhost`, or a `powers.fetch` host. `store(true)` keeps cookies and storage under the plugin data folder. macOS draws it; other systems show one sentence. A page that already lives in the plugin folder is a web panel: `"web": "ui/index.html"` instead of `"view"`, and `"store": true` keeps that page the same way. The page calls `kagan.post(message)` and hears `kagan` events. The script answers with `post(panelId, message)` from `kagan/panel` on a `page` job. `kg ui` sees the panel and cannot drive the page. Use the theme variables from `/__kagan/theme.css`. See `examples/plugins/web-panel`. `writeClipboardPng(base64)` on `kagan/host` writes a PNG when `powers.clipboard.write` is granted, and on Linux it fails because a png clipboard is not available on this platform.

   Drag and drop uses gpui-kit's calls on a `div`, `h_flex` or `v_flex`: `on_drag(payload, handler)` on the source, `on_drag_move` and `on_drop` on each target. The payload is text. A target hears `side` (`"top"`, `"middle"`, `"bottom"`, or `null` when the pointer leaves), and only the innermost target hears a drag. Paint the dragged and active looks yourself from what the handlers hear. Give each source and target an `id(...)`: `kg ui drag <from> <to> --at top` drives it.

5. **Check it.** `kg plugin check <path>` reads the manifest, the entry, and each contribution the same way install does, and reports a refused contribution by name. `kg plugin types <path>` writes `kagan.d.ts` for this version. Run it again after upgrading kagan, and before you call a component you have not seen. `kagan.d.ts` and `gpui-kit.d.ts` are the API of the running app.

6. **Install, and wait for the person.** `kg plugin install <path>` (or `git:<url>@<ref>`) blocks until the person approves the plugin's powers in the app. A declined install fails the command. Do not approve it yourself, with `kg ui` or the operating system.

7. **Test the command and the log.** Run `kg plugin command <id> hello`, then `kg plugin logs <id>`. The line `hello from the starter plugin` means the command ran. `kg plugin logs <id> --follow` streams new lines.

8. **Iterate.** `kg plugin dev <path>` watches a local plugin and reloads it on each change. Find and click the panel's controls with `kg ui snapshot` and `kg ui click <id>`. Check the screen with `kg ui screenshot`.

9. **Share it.** Commit and push the plugin, then run `kg plugin share <path>`. It checks the plugin, refuses uncommitted changes or a commit no remote branch holds, and prints an index entry that pins that commit plus a `kg plugin install git:<url>@<commit> --plugin <folder>` line. Paste the entry into `plugins` in the team index's `.kagan/index.json` (top level: `"schemaVersion": 1` and a lowercase `"name"` (`a-z 0-9 - _ .`, up to 64, as are plugin names; a description is one line); unknown keys are refused; an index in git lists only `git:` sources), then commit and push the index. Teammates run `kg plugin install <name>` or `<index>/<name>`, and each one approves the powers. `kg plugin update <id>` follows the ref in the entry: a commit pins it; a branch lets update follow.

Unavailable host areas fail explicitly. Do not invent an API: if a call is not in `kagan.d.ts`, it does not exist in this version.
