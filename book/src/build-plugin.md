# Build a plugin

A plugin is a folder of JavaScript files. There is no build step. kagan runs the files in the folder.

## Make a starter plugin

```bash
kg plugin new hello
kg plugin check ./hello
kg plugin install ./hello
kg plugin dev ./hello
```

1. `new` makes the folder `hello` with a manifest and a starter view.
2. `check` reads the manifest and the entry file. It does not run the plugin.
3. `install` links the folder. Click **Install** on the card in the window.
4. `dev` loads the plugin again each time that you save a file. It keeps running, so use a second terminal for other commands. A **Hello** tool shows below the threads.

To see errors and log lines, run `kg plugin logs dev.kagan.hello --follow`. To get type files for your editor, run `kg plugin types ./hello`.

To let an agent do the work, click **New plugin…** on the rail. The agent can read the `kg-plugin-authoring` skill.

## The manifest

`kagan.json` tells kagan what the plugin is and what it can do. The starter manifest has these keys:

- `id`: the plugin id, for example `dev.kagan.hello`.
- `name`: the plugin name.
- `version`: the plugin version.
- `kagan`: the oldest kagan version that can load the plugin.
- `entry`: the main script, `main.js`.
- `instructions`: a short text for agents.
- `powers`: what the plugin can do. The starter asks for none.
- `commands`: commands that a person can bind to a key.
- `ui.panels`: `"placement": "left"` puts a tool below the threads. `"placement": "center"` puts a page in the center.
- `tools` and `skills`: agent tools and skills. The starter has none.

kagan refuses a manifest with a key that it does not know. The [plugin API](spec/plugin-api.md) lists all the keys and powers. The [`examples/plugins`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins) folder has small plugins: a panel, a sidebar, a review, an overlay, a `kg` command, and a theme.

## Share it

1. Commit the plugin and push it.
2. In the plugin folder, run `kg plugin share`.

`share` checks the plugin, and makes sure that the remote has the commit. Then it prints two lines:

- An entry for your team index, `.kagan/index.json`.
- A command that a teammate can run without an index: `kg plugin install git:<url>@<commit> --plugin <folder>`.

The entry has the exact commit. To let updates follow a branch, change the commit in the entry to the branch name.

Each teammate approves the plugin on their own computer. [Plugins](plugins.md#install) tells what the install card shows.
