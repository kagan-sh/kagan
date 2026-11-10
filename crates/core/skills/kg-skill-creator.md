---
name: kg-skill-creator
description: Create, review, or fix a kagan skill. Use when the person asks for a reusable instruction folder, a SKILL.md, a skill that triggers on a task, or a skill that should reach agents in kagan threads.
---

Read `kg guide skills` for the live contract.

## Where a skill goes

- Built into kagan: `crates/core/skills` in the kagan source; it ships in the binary.
- A plugin: a folder the plugin lists in `kagan.json` under `skills`.
- User: `<dataDir>/skills`.
- Project: `<project>/.kagan/skills`.

A higher source wins the same name. Two skills with the same name in one source are both dropped, and `kg skill list` reports it. Use `kg skill new <name>` or `kg skill new <name> --project` to scaffold a user or project skill. Only user and project skills can be removed.

## Write the file

`SKILL.md` needs YAML frontmatter:

```yaml
---
name: release-notes
description: Write release notes from the git log. Use when the person asks for release notes, a changelog, or a summary of commits since a tag.
---
```

- `name` is lowercase, hyphenated, and matches the folder. A user or project skill must not start with `kg-`; that prefix is reserved for kagan's bundled and plugin skills.
- `description` says what the skill does and the concrete requests or symptoms that should trigger it. Keep it to one or two sentences. This is the only text the agent sees before it loads the skill.
- `argument-hint` is optional.
- Put the steps after the frontmatter. State the outcome first, then the commands in order, then the checks. Name exact `kg` commands and flags; do not invent an API.
- Move long examples, tables, and reference material into files beside `SKILL.md` and point to them. The agent reads those files only when it loads the skill.
- Do not write a path from one machine (`/Users/...`, `/opt/...`) unless the skill is for that machine. Use the variables kagan sets (`KG_THREAD_ID`, `KG_PROJECT_ID`, `KG_ENVIRONMENT_ID`, `KG_SOCKET`) and the data folder from `kg status --json`.

## Check it

Run and read the output:

```sh
kg skill list
kg skill show <name>
kg skill files <name>
```

`kg skill list` shows duplicate errors. `kg skill show` prints the body the agent gets. `kg skill files` prints the files beside `SKILL.md`.

Then ask the agent to load the skill with `skill_load { "name": "<name>" }` and do the task once. `skill_load` is connected to Claude and Codex. For another program, start a thread and check that the composed native folder contains the skill. Fix the description if the agent did not load it for the request, and fix the steps if it loaded it but did the task wrong.

A bundled, plugin, user, or project skill also reaches Claude, Cursor, Codex, Kimi, OpenCode, and Pi natively: Claude and Cursor as a plugin catalog, Codex from `.agents/skills`, OpenCode from `.opencode/skills`, and Kimi and Pi with a skills-dir or skill flag.
