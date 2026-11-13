---
name: kg-plugin-authoring
description: Build, check, install, and test a kagan plugin with a panel, a command, an agent tool, or a skill. Use when the person asks to build, make, or fix a plugin, or to add a panel, command, tool or skill to kagan. The person approves its powers.
---

The running app is the contract. `kg plugin types <path>` rewrites `kagan.d.ts` and `gpui-kit.d.ts` from it. Run that again after upgrading kagan, and before you call a host function or a component you have not used. Read `references/check.md` for what `kg plugin check` refuses, and what it does not run.

Build a plugin in this order. Each step names the command to run.

1. **Start from the starter.** `kg plugin new <name>` writes a folder that already works: `kagan.json`, `main.js`, `jsconfig.json`, and `kagan.d.ts`. The starter has a left tool `hello` with a `KgButton`, and a command `hello` with no key that writes a line to the plugin log. A fixed key would clash with the next starter, so the person binds one in Settings → Keyboard, or with `kg keymap set <id>:hello <key>`. Add `"header": true` and `"when": "thread"` to draw a command as a button on the thread title. Its id is `dev.kagan.<name>`. `<name>` is lowercase and hyphenated.

2. **Edit the manifest, `kagan.json`.** Required: `id` (reverse-DNS), `name`, `version`, `kagan` (`">=0.1.0"`), and `entry` (`"main.js"`). Optional: `instructions` (one standing text for the agent), `"instruct": true` (a job at each agent start that may append text for that start only), `powers`, `settings`, `commands`, `ui`, `tools`, `cli`, `skills`, `events`, `schedules`, `themes`, `icon`. The contributions, settings, powers, events, schedules and calls are in `references/manifest.md`.

3. **Write the views and jobs in `main.js`.** The entry's default export is a View class whose `init` starts the job loop. Every other View is an export with the name the manifest gives it. Before you write it, read `references/host-and-jobs.md`: the starter loop, the job kinds each one expects, and the host calls.

4. **Use `KgButton` for buttons,** imported from `gpui-component`. A plain `Button` cannot be clicked through `kg ui`, and `kg plugin check` warns about it. Give every control a stable id. Fields, web pages, and drag and drop are in `references/ui-controls.md`.

5. **Check it.** `kg plugin check <path>` reads the manifest, the entry, and each contribution the same way install does, and reports a refused contribution by name. It also refuses a missing host export and a field constructor the component schema rejects. Fix every error before install.

6. **Preview it first.** `kg plugin preview start <path>` starts a hidden kagan with the plugin in it. It blocks until the person approves the plugin's powers in their own window, once. It prints `export KG_DATA_DIR=… KG_SOCKET=…` and a `stop` line. Do not approve it yourself, with `kg ui` or the operating system.
7. **Test in the preview, never in the person's window.** Export the two variables the preview printed, then run the commands against it: `kg plugin command <id> hello`, `kg plugin logs <id>`, `kg ui snapshot`, `kg ui click <id>`, `kg ui screenshot`. The line `hello from the starter plugin` means the command ran. Each save reloads the preview. Its reload errors go to `watch.log` in the preview folder. If your edit adds powers, the reload fails with `powers grew (…)`, and the last approved code keeps running; run `kg plugin preview start <path>` again so the person can approve them. Stop it with `kg plugin preview stop <id>` when you are done.
8. **Install, last.** When the preview works, `kg plugin install <path>` (or `git:<url>@<ref>`) adds the plugin to the person's kagan. It blocks until the person approves the powers again. A declined install fails the command.
9. **Share it.** Commit and push the plugin, then run `kg plugin share <path>`. The index entry, the team index, and how teammates install the plugin are in `references/share.md`.

Unavailable host areas fail explicitly. Do not invent an API: if a call is not in `kagan.d.ts`, it does not exist in this version.

## References

- `references/manifest.md`: every manifest field and contribution.
- `references/host-and-jobs.md`: the job loop, job kinds, and host calls.
- `references/ui-controls.md`: buttons, text fields, web pages, drag and drop.
- `references/check.md`: what `kg plugin types` and `kg plugin check` cover.
- `references/share.md`: publishing a plugin for a team.
