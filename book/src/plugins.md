# Plugins

Plugins are how you extend the IDE. kagan ships with threads, a terminal, review, and the browser. It ships with no plugins.

A plugin can add a tool, a page, a command, an agent tool, a skill, a theme, or one status line. Nothing from a plugin runs until you approve it. An agent cannot approve a plugin.

Small plugins you can install and read are in [`examples/plugins`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins). To write your own, see [Build a plugin](build-plugin.md). Each tool or page has an icon on the rail. [The window](window.md#rail) explains the icons.

## Install

```bash
kg plugin install git:<url>@<ref> --plugin <name>
kg plugin install ./<folder>
```

kagan reads `kagan.json` before any plugin code runs. Questions then shows an install card. Read the powers and click **Install**.

A plugin can ask to:

- Run programs it names.
- Use the network.
- Use the clipboard.
- Read named folders in your home folder, or folders you select in its settings.
- Read and change your threads, or use its own database.
- Open and drive the Browser tab for a thread it names.

It can always read its own folder and change its own data folder. A plugin that can run `git` can use your git credentials, such as SSH keys and a GitHub token.

A plugin from git stays at the commit you installed. A folder install is a link, not a copy. Your changes apply the next time the plugin loads.

## Update

```bash
kg plugin update <id>
```

If the new version asks for more powers, kagan asks you again.

## Install from an index

A team can publish plugin names and git addresses in `.kagan/index.json`, then install by name instead of pasting an address.

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

## What a plugin can change

A **tool** sits below the threads. A **page** replaces the center. A **right panel** is a tab in the right sidebar. Click the rail icon to open it, and click again, or **Close**, to return.

A plugin can offer to draw the thread list, the review, or both. Nothing changes until you pick it in Settings → Appearance → Sidebar or Review.

```bash
kg plugin use threads <id>
kg plugin use review <id>
kg plugin use threads kagan
```

`kagan` is the built-in version. kagan always keeps the rail, New thread, Questions, and the review buttons. Only kagan sends your review comments to the agent.

A plugin can put one short status line at the right of the [activity strip](window.md#activity-strip). You cannot click it. Turn **Plugin status** off in Settings → Appearance to hide every line, or turn that plugin off in Installed. [Build a plugin](build-plugin.md#status-line) shows the manifest.

A plugin can also add a theme. [Themes](themes.md#themes-from-plugins).

[Keyboard shortcuts](keys.md#change-a-key) explains plugin keys. [Troubleshooting](troubleshooting.md#a-plugin-fails) covers logs and recovery.
