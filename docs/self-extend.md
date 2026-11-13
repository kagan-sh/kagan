# kagan self-extend: guide, skills, plugin loop

Status: in progress. Terms follow [CONTEXT.md](CONTEXT.md). Builds on [architecture.md](architecture.md) (`kg`, `Op`) and [plugin-api.md](plugin-api.md) (manifest, `skills`, install).

An agent in a thread can learn kagan, find and load skills, and build, install, reload and debug a plugin, all through `kg`. The tool teaches itself, and the agent never guesses the API.

## Rules

- Everything here is an `Op`, so the app, `kg` and plugins get it alike.
- Every `kg` command takes `--json`. A failure prints the `OpError` envelope `{ "error": { "kind", "message" } }` and exits non-zero.
- An agent-built plugin is installed like any other: a local path, the same manifest, the same powers screen. There is no trust shortcut for agents.

## Thread context

Every thread's program gets `KG_THREAD_ID`, `KG_PROJECT_ID`, `KG_ENVIRONMENT_ID`, `KG_CLI` (the absolute path of the running `kg`), `KG_SOCKET` (the app's socket), and a `PATH` with `kg`'s folder first, so `kg` works by name. It also gets the environment variables you set with `kg env set`, or in Settings (decision 220). `kg status` prints the ids, plus warnings such as a plugin that failed to load. Commands that take a thread, project or environment default to these.

## Guide

`kg guide` prints the concepts (the `CONTEXT.md` terms) and the chapter list. `kg guide <chapter>` prints one chapter: `threads`, `projects`, `environments`, `plugins`, `skills`, `ui`, `customize`, `json`. The text ships in the binary, so it always matches the running version. The generated `kg-plugin-commands` skill lists each plugin command with its manifest `usage` (decision 209). `kg <name> <command> --help` passes `--help` to the plugin, which may answer it.

## Instructions

Kagan does not inject `<dataDir>/AGENTS.md` or `<project>/.kagan/AGENTS.md` into a prompt, and it does not write the project's `AGENTS.md`. The person's program reads its own instruction files. Agent integration (decision 49, amended by 172) adds one fixed note. `<dataDir>/instructions.md`, when it has text, is appended to that note after the line `Instructions from your kagan instructions file:`, and `<project>/.kagan/instructions.md` after `Instructions from this project's kagan instructions file:`. Each file stops at 16 KiB, on a character boundary; a missing or unreadable file adds nothing [decision 211]. The note reaches `claude` as `--append-system-prompt-file`, with `--disallowed-tools Task,Agent`. It reaches `codex` as the `kg mcp` server instructions, with `--disable multi_agent` and `--disable multi_agent_v2`. It reaches `cursor-agent` and `agent` as `<thread folder>/.cursor/rules/kagan.mdc` (`alwaysApply: true`, the file names `KG_THREAD_ID` and not the thread id, one git exclude line for that path) and as a copy in the per-thread plugin. It reaches `pi` as `--append-system-prompt` of `<dataDir>/agent/threads/<id>/note.txt`. `kimi` and `opencode` get the composed skills and no extra instruction file. The composed skill tree reaches each program kagan recognizes for skill injection by its native path (decision 158): `claude` and `cursor-agent` load it as a plugin, `codex` from `.agents/skills`, `opencode` from `.opencode/skills`, `kimi` with `--skills-dir`, and `pi` with `--skill`. A program kagan does not recognize (such as `gemini`, `amp`, `aider`, `goose`, `qwen`, `copilot`, `grok`, or `droid`) gets the `KG_*` variables but no native skills from this path.

## Skills

A skill is a folder with a `SKILL.md`: YAML frontmatter `name` (lowercase, hyphenated, same as the folder) and `description`, then the instructions.

Sources, lowest precedence first:

| source | where |
|---|---|
| bundled | inside the `kg` binary |
| plugin | each enabled plugin's manifest `skills` |
| user | `<dataDir>/skills` |
| project | `<project>/.kagan/skills` |

A higher source overrides a lower one with the same name. A user or project name that starts with `kg-` is dropped, because that prefix belongs to kagan's bundled and plugin skills. Two skills with the same name in one source are both dropped and shown as an error in `kg skill list`.

**Reaching the agent.** `kg mcp` offers the agent tool `skill_load { name }`. The tool list names each skill and its description. The tool returns the body and the skill's absolute file paths. `kg skill files <name>` lists the same paths; the agent reads them with its own file tools. Bundled skills list their supporting files too. They are written to `<dataDir>/agent/bundled/<hash>` (decision 208), so each path is a real file.

`skill_load` covers kagan's four skill sources. It is connected to `claude` and `codex` only, because those are the programs that get the `kg mcp` server.

**Native skills.** The resolved kagan skills (bundled, plugin, user, project) are written to an immutable per-content folder at `<dataDir>/agent/catalogs/<hash>/skills`. Each start resolves the catalog; a changed catalog gets a new hash, so a running thread keeps the folder it started with. `claude` gets that catalog as `--plugin-dir`, so its skills appear as `kagan:<name>`. `cursor-agent` gets `--plugin-dir <dataDir>/agent/threads/<id>/cursor`, which links `skills` to the catalog and keeps a copy of the note in `rules/kagan.mdc` for that thread only. The note the program loads is `<thread folder>/.cursor/rules/kagan.mdc`. `codex` gets a link per skill under `<thread folder>/.agents/skills`, `opencode` under `<thread folder>/.opencode/skills`, `kimi` gets `--skills-dir` for each existing native root and the catalog, and `pi` gets `--skill <catalog>/skills` plus `--append-system-prompt` of the note file. Kagan creates a link only when that name is free, leaves a person's folder or unrelated link alone, removes only links whose target is inside its own agent folder, and adds one `.git/info/exclude` line per link it owns when the folder is a repository. Kagan never writes a program's global config or skills folder. A program kagan does not recognize gets no native skills from this path. Codex and OpenCode have no per-run skills path that kagan uses while keeping their own skills, so their links live in the thread folder and another session of the same program there sees them; for Codex, `.agents/skills` is also read by Cursor, Gemini, Kimi, and OpenCode. Two Codex or OpenCode threads in one folder share the links, so a later start repoints them.

**Commands:** `kg skill open` (shows the Skills page), `kg skill list`, `kg skill show <name>`, `kg skill files <name>`, `kg skill new <name> [--project]` (scaffold into user or project skills), `kg skill add <path> [--project]` (copy a skill folder in), `kg skill update <name> <text>` (replaces a user or project skill's whole `SKILL.md`), `kg skill remove <name>` (user and project skills only).

## Bundled skills

Short, and each points at `kg guide` chapters and the generated `.d.ts` files instead of copying the API, so they do not go stale.

- `kg-cli`: thread context, threads, projects, environments, `--json`.
- `kg-threads`: when a helper is another thread, and how to create, paste, wait, stop, ask, and read one. The note names it. The procedure stays in the skill.
- `kg-plugin-authoring`: manifest, job loop, HostModule areas, panels and spots, Workbench tokens, the dev loop below.
- `kg-plugin-port`: map each surface onto one script and report what moved.
- `kg-skill-creator`: writing and testing a skill.
- `kg-customize`: where a theme, key, preference, agent rule, or skill folder lives. The facts are in `kg guide customize`.
- `kg-ui`: drive the open window with `kg ui`. It cannot approve powers.
- `kg-plugin-commands`: generated, not a file in the binary. One line per `kg` command of each running plugin, from its manifest usage. Present only when a plugin adds a command (decision 209).

## Plugin dev loop

- `kg plugin new <name> [--dir <path>]`: scaffolds `kagan.json`, `main.js` with the job loop, `jsconfig.json`, `kagan.d.ts` and `gpui-kit.d.ts`.
- `kg plugin check [path]`: checks the manifest (unknown keys, schemas, declared views exported) and refuses a script that imports a missing host export or builds a field the component schema rejects, without loading it.
- `kg plugin install <path>`: a local path is linked, not copied, so edits take effect on reload.
- `kg plugin reload <id>`: reloads an installed plugin. `kg plugin dev [path]` is a `kg` loop over `Reload` that watches a local folder and reloads on every change, so a `PluginOp::Dev` call on its own reads the record once and does not watch. There is no build step.
- `kg plugin preview start [path]`, `kg plugin preview stop <id>`, `kg plugin preview list`: run the plugin in a hidden kagan of its own, on its own data folder and socket. The person approves it once in their own window; their window does not change view. `start` prints the two variables `KG_DATA_DIR` and `KG_SOCKET` to put in front of each command that drives it, and a `stop` line [decision 274].
- `kg plugin logs <id> [--follow]`: `host.log` output, crashes, slow marks.
- `kg plugin types [path]`: rewrites the `.d.ts` files for the running version.
- `kg plugin list`, `kg plugin remove <id>`.

**Approval.** Installing, or reloading a local plugin whose powers grew, raises a pending interaction on the thread that ran the command. The person approves the powers screen; the command waits and fails if they decline. A `downloads` key or `powers.spawn` is refused.

**An agent building a plugin:** `kg plugin new` → edit → `kg plugin check` → `kg plugin preview start ./<name>` (the person approves once, in their window) → put the two variables it printed in front of each command that drives it: `kg ui`, `kg thread run`, and the plugin's own commands → read `kg plugin logs` against the preview → when it works, `kg plugin install ./<name>` into the person's kagan (the person approves again). An agent never drives the person's window to test a plugin.

## Ops added

`Status`, `Guide { chapter? }`, `Skill(SkillOp)` (`List`, `Show`, `Files`, `New`, `Add`, `Remove`), and on `PluginOp`: `New`, `Check`, `Reload`, `Logs`, `Types`, `List`, `Remove`. `kg plugin dev` is a client-side loop over `Reload`, and `kg plugin preview` is an Op that starts a hidden kagan.

## Not in v1

A skill registry or search. The plugin marketplace is decision 3 in [decisions.md](decisions.md).

## Themes

Themes ship with the app: `fjord`, the default. `kg theme get` and `kg theme set` choose a theme id and a mode: `light`, `dark`, or `system` (the default, which follows the operating system). `ember` and `grey` ship with the app too. A custom theme is a JSON file in `<dataDir>/themes`; `kg theme new <id>` writes a copy of the theme in use there, and `kg theme folder` prints the folder. A plugin ships themes as files it declares under `themes` (decision 101). The tokens and the file rules are in [look.md](look.md).

Plugin logs use a byte cursor and bounded read batches. The writer keeps `logs.jsonl` and one previous `logs.1.jsonl`, each at most 10 MiB. A cursor beyond the current file returns `reset: true` and starts at 0. A record larger than 10 MiB is rejected.
