# Build a plugin

There is no build step. The files in the folder are what runs.

```bash
kg plugin new hello
kg plugin check hello
kg plugin install hello
kg plugin dev hello
kg plugin logs dev.kagan.hello --follow
kg plugin types hello
```

`new` writes a folder named `hello`. `check` reads the manifest and the entry file. It does not run the plugin. `install` links that folder and waits for you to approve it in the app. `dev` reloads the plugin when a file changes. `types` rewrites the `.d.ts` files for this version of kagan. `logs --follow` shows crashes and its own log lines.

`new` writes this manifest:

```json
{
  "id": "dev.kagan.hello",
  "name": "hello",
  "version": "0.1.0",
  "kagan": ">=0.1.0",
  "entry": "main.js",
  "instructions": "A starter plugin: a hello tool under the threads. A panel may also be a center page. Right and bottom are refused. A hello command writes to its log; a person binds its key in Settings, Keyboard.",
  "powers": {},
  "commands": [
    {
      "id": "hello",
      "title": "Say hello"
    }
  ],
  "ui": {
    "panels": [
      {
        "id": "hello",
        "title": "Hello",
        "view": "HelloPanel",
        "placement": "left"
      }
    ]
  },
  "tools": [],
  "skills": []
}
```

`id` is the plugin's name in commands and on disk. `kagan` is the oldest app version that can load it. `entry` is the script. The starter adds a left sidebar tool and a command that writes to its log. A panel can instead be a center page; right and bottom panels are not supported. `"ui": { "overlay": "<View>" }` draws a view over the terminal that may only paint, so it takes no clicks, keys or focus; `examples/plugins/overlay` shows one. A panel can also offer to replace the thread list or the review; see [Plugins](plugins.md#replaceable-sidebar-and-review). `secondary` means Command on macOS and Control plus Shift on other platforms; an extra `shift` does not add another key. Unknown manifest keys are an error.

The entry file contains the `HelloPanel` view and a job loop that handles the hello command. See the [plugin API](spec/plugin-api.md) for the available powers and jobs.

Choose New plugin… on the rail to start a Personal thread with a prompt ready to finish. The agent can read the instructions with `kg skill show kg-plugin-authoring`.

## Share it

Commit the plugin and push it. Then, from its folder:

```bash
kg plugin share
```

`share` checks the plugin, makes sure the folder has no changes and that a remote branch holds the commit, and prints two things:

- an entry to paste into `plugins` in your team's index, `.kagan/index.json`
- a line a teammate can run without an index: `kg plugin install git:<url>@<commit> --plugin <folder>`

The entry pins the exact commit you checked. A password in the remote address is left out. Teammates who added the index run `kg plugin install <name>`, or `kg plugin install <index>/<name>` to pick one index.

Each teammate still approves the plugin. The card names the address, the ref, the full commit and the powers in plain words. It adds a caution when the code is a folder on their computer, or comes from a host and owner, such as `github.com/acme`, where they added no index. Nothing runs before they press Install. An update asks again when the powers grew.

`kg plugin update` follows the ref in the entry. A commit pins the plugin; to let updates follow a release branch, change the entry's ref to that branch.

