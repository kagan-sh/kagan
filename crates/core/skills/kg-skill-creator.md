---
name: kg-skill-creator
description: Create, review, or fix a kagan skill. Use when the person asks for a reusable instruction folder, a SKILL.md, a skill that triggers on a task, or a skill that should reach agents in kagan threads.
---

Read `kg guide skills` for the live contract.

## Where a skill goes

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

`skill_load` is connected to Claude and Codex. For another program, start a thread and check that the composed native folder contains the skill.

## Test it

Use a fresh thread for each test, so earlier context does not help the agent.

1. Ask for a task in the words that should load the skill. Check that the agent calls `skill_load { "name": "<name>" }`. If it did not, fix the `description`. If it loaded the skill but did the task wrong, fix the steps.
2. Ask a near miss: a request close to the skill's topic that should not load it. Check that the agent does not load it. If it did, narrow the `description`.
3. Do the same task with the skill, then without it or with the old version. Keep the version that does better, and say why in one line.

A bundled, plugin, user, or project skill also reaches Claude, Cursor, Codex, Kimi, OpenCode, and Pi natively: Claude and Cursor as a plugin catalog, Codex from `.agents/skills`, OpenCode from `.opencode/skills`, and Kimi and Pi with a skills-dir or skill flag.
