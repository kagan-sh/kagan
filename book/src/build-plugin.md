# Build a plugin

A plugin is a folder of JavaScript files. kagan runs the files directly. There is no build step.

## Make a starter plugin

```bash
kg plugin new hello
kg plugin check ./hello
kg plugin install ./hello
kg plugin dev ./hello
```

1. `new` makes the `hello` folder with a manifest and a starter view.
2. `check` reads the manifest and the entry file. It does not run the plugin.
3. `install` links the folder. Click **Install** on the card in the window.
4. `dev` loads the plugin again after each save. Use a second terminal for other commands. A **Hello** tool shows below the threads.

Run `kg plugin logs dev.kagan.hello --follow` to see errors and log lines. Run `kg plugin types ./hello` to get type files for your editor.

To let an agent do the work, click **New plugin…** on the rail. The agent can read the `kg-plugin-authoring` skill.

## The manifest

`kagan.json` describes the plugin and its powers. The starter manifest has these keys:

- `id`: the plugin id, for example `dev.kagan.hello`.
- `name`: the plugin name.
- `version`: the plugin version.
- `kagan`: the oldest kagan version that can load the plugin.
- `entry`: the main script, `main.js`.
- `instructions`: a short text for agents.
- `powers`: what the plugin can do. The starter asks for none.
- `commands`: commands that you can bind to a key.
- `ui.panels`: `"placement": "left"` puts a tool below the threads. `"placement": "center"` puts a page in the center.
- `tools` and `skills`: agent tools and skills. The starter has none.

kagan refuses a manifest with an unknown key. Run `kg plugin check ./hello` to find manifest errors. The [`examples/plugins`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins) folder has examples that work.

## Share it

1. Commit the plugin and push it.
2. In the plugin folder, run `kg plugin share`.

`share` checks the plugin and makes sure that the remote has the commit. It prints:

- An entry for your team index, `.kagan/index.json`.
- A command that a teammate can run without an index: `kg plugin install git:<url>@<commit> --plugin <folder>`.

The entry has the exact commit. To let updates follow a branch, change the commit in the entry to the branch name.

Each teammate approves the plugin on their own computer. [Plugins](plugins.md#install) explains the install card.

## Get listed on kagan.sh

To list your plugin on [kagan.sh/plugins](https://kagan.sh/plugins/), add the topic `kagan-plugin` to its GitHub repository. Nobody reviews the listings. Every install still shows the plugin's powers and asks first.
