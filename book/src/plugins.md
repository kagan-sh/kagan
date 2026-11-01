# Plugins

A plugin is a folder of JavaScript. It can add a tool below the threads, a page in the center, commands, agent tools, skills, or themes. kagan has no plugins when you install it. The [`examples/plugins`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins) folder has small plugins that you can install and read.

Each installed plugin with a tool or a page has an icon on the rail. [The window](window.md#rail) tells you how the icons work.

## Install

Install a plugin from git, or from a folder on your computer:

```bash
kg plugin install git:<url>@<ref> --plugin <name>
kg plugin install ./<folder>
```

kagan reads the plugin manifest, `kagan.json`, before it runs any plugin code. Then the window shows an install card in Questions. The card lists the powers of the plugin. Nothing runs before you click **Install**. Each plugin runs in its own JavaScript engine, and kagan gives it only the powers that you approve. Only a person in the window can click it. An agent cannot approve a plugin.

A plugin can ask for these powers:

- Run programs that it names.
- Use the network.
- Use the clipboard.
- Read folders in your home folder that it names, or folders that you select in its settings.
- Read and change your threads, or use its own database.

Each plugin can always read its own folder, and read and write its own data folder. A plugin that can run `git` can use your git credentials, such as SSH keys and a GitHub token.

A plugin from git stays at the commit that you installed. A plugin from a folder is linked, not copied, so your changes apply when it loads again.

## Update

```bash
kg plugin update <id>
```

If the new version asks for more powers, kagan asks you again.

## Install from an index

A team can publish its plugins in an index. An index is a list of plugin names and their git addresses, in the file `.kagan/index.json`. Add the index one time, then install by name:

```bash
kg plugin index add git:<url>@<ref>
kg plugin search <text>
kg plugin install <name>
```

- `kg plugin index list` shows your indexes. kagan searches the first index first.
- `kg plugin index remove <address>` removes an index.
- `kg plugin install <index>/<name>` installs from one index.
- Settings → Plugins → Indexes does the same in the window.

The install card shows the git address, the commit, and the index. It shows a caution when the code is a folder on your computer, or comes from an owner whose index you did not add. kagan saves the git address and the commit, not the name. A later change to the index does not change your plugin.

## Turn on, turn off, remove

Settings → Plugins has a switch for each plugin. A plugin that is off does not load.

```bash
kg plugin list
kg plugin enable <id>
kg plugin disable <id>
kg plugin remove <id>
```

A plugin id looks like `dev.kagan.hello`.

## Replace the thread list or the review

A plugin can offer to draw the thread list, the review, or both. Nothing changes until you select it in Settings → Appearance → Sidebar or Review. From a terminal:

```bash
kg plugin use threads <id>
kg plugin use review <id>
kg plugin use threads kagan
```

`kagan` is the built-in version. kagan always keeps the rail, New thread, Questions, and the review buttons. Only kagan sends your review comments to the agent.

If the plugin fails or is turned off, kagan goes back to the built-in version immediately. To go back yourself, press Cmd+Alt+B for the thread list or Cmd+Alt+V for the review.

## Keys

Settings → Keyboard lists the commands of each plugin under the plugin name. Click **Bind** or **Change** to set a key.

```bash
kg keymap get
kg keymap set <id>:<command> <key>
kg keymap remove <id>:<command>
```

## Logs

Each plugin card has a **Logs** button. From a terminal:

```bash
kg plugin logs <id>
kg plugin logs <id> --follow
```

`--follow` keeps the log open and prints new lines.
