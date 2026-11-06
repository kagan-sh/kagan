# Plugins

A plugin is a folder of JavaScript. It can add window parts, commands, agent tools, skills, or themes. kagan has no plugins when you install it. The [`examples/plugins`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins) folder has small plugins that you can install and read.

Each plugin tool or page has an icon on the rail. [The window](window.md#rail) explains the icons.

## Install

Install a plugin from git or from a folder on your computer:

```bash
kg plugin install git:<url>@<ref> --plugin <name>
kg plugin install ./<folder>
```

kagan reads the plugin manifest, `kagan.json`, before any plugin code runs. The window then shows an install card in Questions. Read the listed powers, then click **Install**. Nothing runs before you approve. An agent cannot approve a plugin.

A plugin can ask for these powers:

- Run programs that it names.
- Use the network.
- Use the clipboard.
- Read named folders in your home folder, or folders that you select in its settings.
- Read and change your threads, or use its own database.
- Open and drive the Browser tab for a thread it names.

Each plugin can always read its own folder and change its own data folder. A plugin that can run `git` can use your git credentials, such as SSH keys and a GitHub token.

A plugin from git stays at the commit that you installed. For a folder install, kagan links the folder instead of a copy. Your changes apply when the plugin loads again.

## Update

```bash
kg plugin update <id>
```

If the new version asks for more powers, kagan asks you again.

## Install from an index

A team can publish an index of plugin names and git addresses in `.kagan/index.json`. Add the index one time, then install by name:

```bash
kg plugin index add git:<url>@<ref>
kg plugin search <text>
kg plugin install <name>
```

- `kg plugin index list` shows your indexes. kagan searches the first index first.
- `kg plugin index remove <address>` removes an index.
- `kg plugin install <index>/<name>` installs from one index.
- Settings → Plugins → Indexes does the same in the window.

The install card shows the git address, the commit, and the index. It shows a caution for a local folder or an owner whose index you did not add. kagan saves the git address and the commit, not the name. A later index change does not change your plugin.

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

A plugin can offer to draw the thread list, the review, or both. Nothing changes until you select it in Settings → Appearance → Sidebar or Review.

```bash
kg plugin use threads <id>
kg plugin use review <id>
kg plugin use threads kagan
```

`kagan` selects the built-in version. kagan always keeps the rail, New thread, Questions, and the review buttons. Only kagan sends your review comments to the agent.

[Keyboard shortcuts](keys.md#change-a-key) explains plugin keys. [Troubleshooting](troubleshooting.md#a-plugin-fails) covers logs and recovery.

## Add a status line

A plugin can put one short line in the [activity strip](window.md#activity-strip). Declare it in `kagan.json`:

```json
"ui": { "status": { "title": "Usage" } }
```

The title is 1 to 40 characters, and it is the label shown before the value. Then call `setStatus` from `kagan/host`:

```js
import { setStatus } from "kagan/host";

setStatus("42% used", { tone: "busy" });
```

The text is one plain line of at most 120 characters. `tone` is `quiet`, `busy`, `needs`, or `failed`, and defaults to `quiet`. Call `setStatus(null)` to hide the line. A plugin that declares `ui.status` starts with kagan, so the line appears without opening a panel. It is text only: a plugin cannot put a button, click, or view in the strip. If it needs an action, declare a command. Turn **Plugin status** off in Settings → Appearance, or turn the plugin off in Installed, to hide the line.

## Call another plugin

A plugin can run a function another plugin offers. The caller lists the other plugin in `powers.calls`, and the install screen says it asks that plugin to do work. The other plugin lists the function name in `exports`. The caller then uses `call(plugin, name, input)` from `kagan/host`, and gets the reply as JSON. A call that is not approved, names a disabled plugin, or names a function that is not exported is refused. A call times out after 30 seconds. See decision 155.

## Thread events

A plugin lists the events it wants in `kagan.json`. `thread.turn.started` fires when a program's title shows a spinner as its first character. `thread.turn.ended` fires when that spinner ends, or the program stops or exits during it. The end says `completed`, `interrupted`, or `failed`. Quitting does not write `thread.turn.ended`. A working mark later in the title, such as an hourglass after a plain prefix, does not start a turn. `thread.tool.started` fires when kagan hands an agent tool call to a plugin, and carries the tool's name and kagan's own id; `thread.tool.ended` fires when that call's reply lands, with the same id and `ok` or `error`. Neither event holds the input, the reply, an error text, or a plugin id.
