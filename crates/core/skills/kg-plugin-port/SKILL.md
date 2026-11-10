---
name: kg-plugin-port
description: Port a bb plugin into one kagan plugin, map every surface, and report what matches, what is close, and what cannot move.
---

Port one bb plugin into one kagan plugin. Keep the behavior the person sees. Drop a surface only when this map says it cannot move, and say so in the report.

kagan runs one script. `kagan.json` declares every contribution. The person approves powers. Read `kg-plugin-authoring` for the kagan side, and `kagan.d.ts` after `kg plugin types`. A call that is not in that file does not exist. Do not invent one.

## Workflow

1. **Read the bb plugin.** Open `package.json` and the files `bb.server`, `bb.app`, and `bb.host` point at. Read each folder under `bb.skills` (default `skills/`). Note `engines.bb`.

2. **Inventory every surface.** Walk the factory and the web entry. Write down each item below, or "none":
   - Manifest: id, name, description, icon, logo, engines, skills, themes
   - CLI commands and how `argv` is parsed
   - Agent tools, instruction text, and any per-thread tool selection
   - Panels, tabs, overlays, settings views, composer controls, and other web slots
   - Settings and whether a value is secret
   - Key-value storage and the plugin database
   - Events, hooks, HTTP routes, RPC, realtime, schedules, and background services
   - Programs, network hosts, clipboard, and files outside the plugin folder
   - A host entry, a provider, or an environment provider

3. **Map each item** with the table in `references/mapping.md`. Mark it direct, close, or a gap. A gap keeps its workaround, or "cannot move".

4. **Scaffold.** `kg plugin new <name>` writes `kagan.json`, `main.js`, `jsconfig.json`, and the type files. `<name>` is lowercase and hyphenated. The id is `dev.kagan.<name>`. Replace the starter. Point `--dir` at the folder you want.

5. **Port the code** into that one script. The default export is a View whose `init` runs the job loop. Other views are named exports. Start files with `// @ts-check`. Use `KgButton` from `gpui-component`, not `Button`. A field `kg ui` types into is `KgInput` or `KgTextarea` from `gpui-component`, with `aria_label`. In `init`, `InputState.new(placeholder?, initial?)` takes two optional strings and `TextareaState.new(initial?)` takes one. `gpui-base` has the same names with an options object; `KgInput` and `KgTextarea` reject that state. A local page in a panel is `new WebView(id).url(url)`. Give every control a stable id.

6. **Check.** `kg plugin check <path>` reads the manifest and the entry the same way install does, and refuses an import the host does not export and a field constructor the component schema rejects. Fix every error. A warning about `Button` means switch that control to `KgButton`. Run `kg plugin types <path>` again after you call something new.

7. **Install and try it.** `kg plugin install <path>` waits until the person approves the powers. Do not approve it yourself. Then `kg plugin dev <path>`. Try each kind you declared: `kg plugin command <id> <cmd>`, `kg <cli> <sub> …`, and a tool from inside a thread. Read `kg plugin logs <id>`. A trial that must not touch the person's app sets `KG_DATA_DIR` to a new temp directory. The socket follows that folder unless `KG_SOCKET` is set. Stop the headless process the trial started when it is done.

8. **Report** the checklist at the end, filled in. The template is `references/checklist.md`.

## References

- `references/job-loop.md`: the job loop, job kinds, and timing.
- `references/mapping.md`: each bb surface and its kagan match.
- `references/events.md`: event kinds, and examples to copy.
- `references/themes-and-right-panels.md`: theme files and right panels.
- `references/powers.md`: the powers to ask for.
- `references/checklist.md`: the report to fill in.
