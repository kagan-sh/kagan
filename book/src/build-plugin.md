# Build a plugin

A plugin is how you extend the IDE yourself. It is a folder of JavaScript. kagan runs the files as they are. There is no build step.

**New plugin…** on the rail starts a Personal thread with your first installed agent. It pastes the start of a prompt, `Build a kagan plugin that`, and leaves it for you to finish. The agent can read the `kg-plugin-authoring` skill. You can also create the folder yourself.

## Make a starter plugin

```bash
kg plugin new hello
kg plugin check ./hello
kg plugin install ./hello
kg plugin dev ./hello
```

1. `new` makes the `hello` folder, with a manifest and a starter view.
2. `check` reads the manifest and the entry file. It does not run the plugin.
3. `install` links the folder. Click **Install** on the card.
4. `dev` loads the plugin again after each save. Use a second terminal for other commands. A **Hello** tool shows below the threads.

`kg plugin logs dev.kagan.hello --follow` shows errors and log lines. `kg plugin types ./hello` writes type files for your editor.

## The manifest

`kagan.json` describes the plugin and its powers. The starter has these keys:

- `id`: the plugin id, such as `dev.kagan.hello`.
- `name`: the name.
- `version`: the version.
- `kagan`: the oldest kagan that can load it.
- `entry`: the main script, `main.js`.
- `instructions`: a short standing text for agents.
- `instruct`: when `true`, the plugin gets a job each time a program starts, and may append text for that start only.
- `schedules`: runs a job every so many minutes while kagan is open, for example `[{ "id": "sync", "every": 15 }]`. kagan loads the plugin for each run. A missed run is not made up.
- `powers`: what it can do. The starter asks for none.
- `commands`: commands you can bind to a key. `"header": true` with `"when": "thread"` also draws the command on the thread title. `"menu": true` with `"when": "thread"` also lists it in the thread actions menu.
- `ui.panels`: `"placement": "left"` is a tool below the threads. `"placement": "center"` is a page in the center. `"placement": "right"` is a tab in the right sidebar. A panel names `"view"` (a class the entry exports) or `"web"` (an HTML file in the plugin, such as `ui/index.html`). A web panel draws that file. The page calls `kagan.post` and the script answers with `kagan/panel`. See `examples/plugins/web-panel`.
- `tools` and `skills`: agent tools and skills. The starter has none.
- `ui.status`: one plain line in the [activity strip](window.md#activity-strip). See [Status line](#status-line).

kagan refuses a manifest with an unknown key. `kg plugin check ./hello` reports those errors. [docs/plugin-api.md](https://github.com/kagan-sh/kagan/blob/main/docs/plugin-api.md) lists every key, power, and the JavaScript API.

[`examples/plugins`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins) has a panel, a web panel, a sidebar, a review, an overlay, a header button, a `kg` command, a theme, and a schedule that runs every minute.

## Status line

A plugin can put one short line at the right of the activity strip. In `kagan.json`:

```json
"ui": { "status": { "title": "Usage" } }
```

The title is 1 to 40 characters. It is the label before the value. Then, from `kagan/host`:

```js
import { setStatus } from "kagan/host";

setStatus("42% used", { tone: "busy" });
```

The text is one plain line of at most 120 characters. `tone` is `quiet`, `busy`, `needs`, or `failed`, and defaults to `quiet`. `setStatus(null)` hides the line.

A plugin that declares `ui.status` starts with kagan, so the line shows without opening a panel. It is text only: no button, click, or view. A command is how it gets an action. Turn **Plugin status** off in Settings → Appearance, or turn the plugin off in Installed, to hide the line.

## Call another plugin

A plugin can ask another plugin to run a function and return JSON.

The caller lists that plugin in `powers.calls`. The install screen says it asks that plugin to do work. The other plugin lists the function name in `exports`. The caller uses `call(plugin, name, input)` from `kagan/host`. A call is refused when it is not approved, the other plugin is disabled, or the function is not exported. A call times out after 30 seconds.

## Thread events

A plugin can react when an agent turn or a tool call starts and ends. List the events it wants in `kagan.json`.

- `thread.turn.started` fires when a program's title shows a spinner as its first character.
- `thread.turn.ended` fires when that spinner ends, or the program stops or exits during it. The end says `completed`, `interrupted`, or `failed`. Quitting does not write `thread.turn.ended`. A working mark later in the title, such as an hourglass after a plain prefix, does not start a turn.
- `thread.tool.started` fires when kagan hands an agent tool call to a plugin. It carries the tool's name and kagan's own id.
- `thread.tool.ended` fires when that reply lands, with the same id and `ok` or `error`. Neither event holds the input, the reply, an error text, or a plugin id.

## Share it

1. Commit the plugin and push it.
2. In the plugin folder, run `kg plugin share`.

`share` checks the plugin and checks that the remote has the commit. It prints an entry for your team index, `.kagan/index.json`, and a command a teammate can run without an index: `kg plugin install git:<url>@<commit> --plugin <folder>`.

The entry has the exact commit. To let updates follow a branch, change that commit to the branch name.

Each teammate approves the plugin on their own computer. [Plugins](plugins.md#install) explains the install card.

## Get listed on kagan.sh

To list a plugin on [kagan.sh/plugins](https://kagan.sh/plugins/), add the topic `kagan-plugin` to its GitHub repository. Nobody reviews the listings. Every install still shows the powers and asks first.
