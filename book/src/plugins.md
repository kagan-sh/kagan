# Plugins

A plugin is a folder of JavaScript. It can add window parts, commands, agent tools, skills, or themes. kagan has none when you install it. Small ones you can install and read are in [`examples/plugins`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins).

Each tool or page has an icon on the rail. [The window](window.md#rail) explains the icons.

## Install

```bash
kg plugin install git:<url>@<ref> --plugin <name>
kg plugin install ./<folder>
```

kagan reads `kagan.json` before any plugin code runs. Questions then shows an install card. Read the powers and click **Install**. Nothing runs before you approve. An agent cannot approve a plugin.

A plugin can ask to:

- Run programs it names.
- Use the network.
- Use the clipboard.
- Read named folders in your home folder, or folders you select in its settings.
- Read and change your threads, or use its own database.

It can always read its own folder and change its own data folder. A plugin that can run `git` can use your git credentials, such as SSH keys and a GitHub token.

A plugin from git stays at the commit you installed. A folder install is a link, not a copy. Your changes apply the next time the plugin loads.

## Update

```bash
kg plugin update <id>
```

If the new version asks for more powers, kagan asks you again.

## Install from an index

A team can publish plugin names and git addresses in `.kagan/index.json`. Add that index once, then install by name:

```bash
kg plugin index add git:<url>@<ref>
kg plugin search <text>
kg plugin install <name>
```

- `kg plugin index list` shows your indexes. kagan searches the first index first.
- `kg plugin index remove <address>` removes an index.
- `kg plugin install <index>/<name>` installs from one index.
- Settings → Plugins → Indexes does the same.

The install card shows the git address, the commit, and the index. It shows a caution for a local folder, or for an owner whose index you did not add. kagan saves the git address and the commit, not the name. A later change to the index does not change your plugin.

## Enable, disable, or remove

Settings → Plugins has a switch for each plugin. A disabled plugin does not load.

```bash
kg plugin list
kg plugin enable <id>
kg plugin disable <id>
kg plugin remove <id>
```

A plugin id looks like `dev.kagan.hello`.

## Replace the thread list or the review

A plugin can offer to draw the thread list, the review, or both. Nothing changes until you pick it in Settings → Appearance → Sidebar or Review.

```bash
kg plugin use threads <id>
kg plugin use review <id>
kg plugin use threads kagan
```

`kagan` is the built-in version. kagan always keeps the rail, New thread, Questions, and the review buttons. Only kagan sends your review comments to the agent.

[Keyboard shortcuts](keys.md#change-a-key) explains plugin keys. [Troubleshooting](troubleshooting.md#a-plugin-fails) covers logs and recovery.

## Add a status line

A plugin can put one short line in the [activity strip](window.md#activity-strip). In `kagan.json`:

```json
"ui": { "status": { "title": "Usage" } }
```

The title is 1 to 40 characters. It is the label before the value. Then, from `kagan/host`:

```js
import { setStatus } from "kagan/host";

setStatus("42% used", { tone: "busy" });
```

The text is one plain line of at most 120 characters. `tone` is `quiet`, `busy`, `needs`, or `failed`, and defaults to `quiet`. `setStatus(null)` hides the line. A plugin that declares `ui.status` starts with kagan, so the line shows without opening a panel. It is text only: no button, click, or view. A command is how it gets an action. Turn **Plugin status** off in Settings → Appearance, or turn the plugin off in Installed, to hide the line.

## Call another plugin

A plugin can run a function another plugin offers. The caller lists that plugin in `powers.calls`. The install screen says it asks that plugin to do work. The other plugin lists the function name in `exports`. The caller uses `call(plugin, name, input)` from `kagan/host` and gets JSON back. A call is refused when it is not approved, the other plugin is disabled, or the function is not exported. A call times out after 30 seconds. See decision 155.

## Thread events

A plugin lists the events it wants in `kagan.json`.

`thread.turn.started` fires when a program's title shows a spinner as its first character. `thread.turn.ended` fires when that spinner ends, or the program stops or exits during it. The end says `completed`, `interrupted`, or `failed`. Quitting does not write `thread.turn.ended`. A working mark later in the title, such as an hourglass after a plain prefix, does not start a turn.

`thread.tool.started` fires when kagan hands an agent tool call to a plugin. It carries the tool's name and kagan's own id. `thread.tool.ended` fires when that reply lands, with the same id and `ok` or `error`. Neither event holds the input, the reply, an error text, or a plugin id.
