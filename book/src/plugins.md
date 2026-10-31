# Plugins

A plugin is a folder of JavaScript that adds screens, commands, agent tools, or skills. kagan reads its manifest, `kagan.json`, before it runs any plugin code.

Installed plugins appear as icons on the rail at the left edge. A tool's icon shows its body below the threads in the list, and clicking that icon again closes it. A page's icon opens the page in the center. Choosing that icon again, or Close, returns to the terminal. Manage plugins opens Settings → Installed.

## Install

From git, or from a folder on disk:

```bash
kg plugin install git:<url>@<ref> --plugin <name>
kg plugin install <path>
```

A local folder is linked, not copied, so edits apply on reload. Install records the git commit. The same code runs until you update.

The window asks you to approve the install. A card in the right panel lists what the plugin wants to do, with **Install** and **Cancel** buttons. Only a person in the app can answer. An agent cannot approve its own plugin. Cancel makes the command fail.

A plugin can ask to run named programs, make network requests, use the clipboard, read named folders under your home, and access kagan's threads or its own database. It can also read its own folder and read and write its own data folder; its key-value storage is always available. Folder settings can grant read access to folders you choose. Running a program can be granted the full environment. The [plugin API](spec/plugin-api.md) describes the limits and all available powers. A `downloads` key or `powers.spawn` is refused.

A plugin allowed to run git can use your git credentials, including SSH keys, your credential helper, and a GitHub token.

## Find plugins in an index

A team can publish its plugins in an index: a list of names, each with its git address. Add your team's index once, then install by name:

```bash
kg plugin index add git:<url>@<ref>
kg plugin search <text>
kg plugin install <name>
```

An index is the file `.kagan/index.json` in a git repository, or a file or folder on this computer. A private repository uses your own git sign-in. `kg plugin index list` shows your indexes, first searched first, and `kg plugin index remove <address>` drops one. Settings → Indexes does the same: add an index, search, and press Install.

`kg plugin install <index>/<name>` picks one index. Two indexes cannot share a name. Install by name is the same install. The window asks you to approve. The card names the git address, the ref, the full commit and the index. It adds a caution when the code is a folder on your computer, or comes from a host and owner, such as `github.com/acme`, where you added no index. kagan keeps the git address and commit, not the name, so a later change to the index does not change what you installed. Update follows the ref you installed. If an index cannot be reached, search says so and still shows the others.

Update asks again when the powers grew:

```bash
kg plugin update <id>
```

## Replaceable sidebar and review

A plugin can offer to draw the thread list, the review, or both. Nothing changes until you pick it: Settings → Appearance → Sidebar or Review, or

```bash
kg plugin use threads <id>
kg plugin use review <id>
kg plugin use threads kagan
```

`kagan` is the built-in. From a terminal, choosing a plugin asks you in the window first. Installing, updating or reloading a plugin never switches it. The rail, New thread, questions, approvals and counts stay kagan's, and so does the review footer: only kagan sends your review notes to the agent or discards them.

If the plugin stops working, is turned off or removed, or draws something that floats above the window, the built-in comes back at once and the plugin's log says why. Press Cmd+Alt+B (Use built-in sidebar) or Cmd+Alt+V (Use built-in review) any time, even while the plugin holds the keyboard. Off macOS the keys are Ctrl+Shift+Alt+B and Ctrl+Shift+Alt+V.

A plugin with the threads power can read your threads' changed files and start programs in their folders. Choose a replacement you trust as you would any plugin with that power.

## Enable and disable

Settings → Installed has a switch on each plugin. Off means the plugin is not loaded.

```bash
kg plugin list
kg plugin enable <id>
kg plugin disable <id>
kg plugin remove <id>
```

The id looks like `dev.kagan.hello`.

## Keys

A plugin's commands are listed in Settings → Keyboard under the plugin's name, with a key or Not set, and Bind, Change and Remove. Disabling or removing the plugin takes its rows away.

```bash
kg keymap get
kg keymap set <id>:<command> <key>
kg keymap remove <id>:<command>
```

## Logs

Each plugin card has a Logs button. From a terminal:

```bash
kg plugin logs <id>
kg plugin logs <id> --follow
```

`--follow` keeps printing. A rotated log starts again at the beginning and says so.
