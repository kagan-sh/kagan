---
name: kg-plugin-authoring
description: Build, check, install, and test a kagan plugin with a panel, a command, an agent tool, or a skill. Use when the person asks to build, make, or fix a plugin, or to add a panel, command, tool or skill to kagan. The person approves its powers.
---

The running app is the contract: `kg plugin types <path>` rewrites `kagan.d.ts` and `gpui-kit.d.ts`, so run it again after upgrading kagan and before you call a host function or component you have not used. `references/check.md` says what `kg plugin check` refuses, and what it does not run.

Build a plugin in this order. Each step names the command to run.

1. **Start from the starter.** `kg plugin new <name>` writes a folder that already works: `kagan.json`, `main.js`, `jsconfig.json` and `kagan.d.ts`. It has a left tool `hello` with a `KgButton`, and a keyless command `hello` that writes a line to the plugin log. A fixed key would clash with the next starter, so the person binds one in Settings → Keyboard or with `kg keymap set <id>:hello <key>`. `"header": true` with `"when": "thread"` draws a command as a button on the thread title. The id is `dev.kagan.<name>`, lowercase and hyphenated.

2. **Edit the manifest, `kagan.json`.** Required: `id` (reverse-DNS), `name`, `version`, `kagan` (`">=0.1.0"`), and `entry` (`"main.js"`). Optional: `instructions`, `"instruct": true`, `powers`, `settings`, `commands`, `ui`, `tools`, `cli`, `skills`, `events`, `schedules`, `themes`, `icon`. Every contribution, setting, power, event, schedule and call is in `references/manifest.md`.

3. **Write the views and jobs in `main.js`.** The entry's default export is a View whose `init` starts the job loop; every other View is an export named as the manifest says. `references/host-and-jobs.md` has the starter loop, the job kinds and the host calls.

4. **Use `KgButton`,** from `gpui-component`, for every button: a plain `Button` cannot be clicked through `kg ui` and `kg plugin check` warns about it. Give every control a stable id.

4b. **Draw or extend the chat.** A `center` panel with `"replaces": "chat"` and `"follows": "thread"`, granted by `"areas": ["chat"]`, draws an ACP thread's transcript with `kagan/chat`; a command with `"prompt": true` sits before Send. Read `references/chat.md`.

5. **Check it.** `kg plugin check <path>` reads the manifest, the entry, and each contribution the same way install does, and reports a refused contribution by name. It also refuses a missing host export and a field constructor the component schema rejects. Fix every error before install.

6. **Preview it first.** `kg plugin preview start <path>` starts a hidden kagan with the plugin and blocks until the person approves it once, in their window.
7. **Test in the preview, never in the person's window.** Put the `KG_DATA_DIR` and `KG_SOCKET` it prints before each command: `… kg ui snapshot`, `… kg ui click <id>`, `… kg plugin logs <id>`. A save reloads it and errors go to `watch.log`; added powers fail the reload with `powers grew`, so start the preview again. There, `kg thread run` takes `--project-id personal --environment personal`. Stop with `kg plugin preview stop <id>`, no variables.
8. **Install, last.** `kg plugin install <path>` runs without the variables and adds it to the person's kagan. The person approves again.
9. **Share it.** Commit and push the plugin, then run `kg plugin share <path>`. The index entry, the team index, and how teammates install the plugin are in `references/share.md`.

Unavailable host areas fail explicitly. Do not invent an API: if a call is not in `kagan.d.ts`, it does not exist in this version.

Never answer an approval; never approve powers; a chat plugin draws, the host types and answers.

## References

In the `references` folder: `manifest.md` (every field and contribution), `host-and-jobs.md` (the job loop, job kinds, host calls), `ui-controls.md` (buttons, fields, web pages, drag and drop), `chat.md` (a chat's transcript and prompt row), `check.md` (what `types` and `check` cover), `share.md` (publishing for a team).
