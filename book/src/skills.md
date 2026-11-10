# Skills

A skill is a folder of instructions an agent can load for one kind of task. You use skills to teach an agent a job you repeat, such as writing release notes or building a plugin. The folder has a `SKILL.md`:

```markdown
---
name: release-notes
description: Write release notes from the git log.
---

Read the commits since the last tag. Group them by feature and fix.
```

The lines between `---` are the `name` and the `description`. The name is lowercase letters and hyphens, and it matches the folder name. The rest is the instructions.

The agent sees each name and description, and loads the instructions only when it needs them.

## Manage skills

```bash
kg skill list
kg skill show kg-plugin-authoring
kg skill new my-skill
kg skill new my-skill --project
kg skill add <path>
kg skill remove my-skill
```

- `new` makes a folder in your skills. `--project` makes it in the project.
- `add` copies a skill folder you already have.
- `remove` deletes one of your skills or a project skill. It cannot remove a built-in skill or a plugin skill.
- A name that starts with `kg-` is reserved. `new` and `add` refuse it.

## Where skills come from

Four places. The same name in two places: the later place wins.

1. Built into `kg`.
2. An enabled plugin.
3. Your skills: `<data folder>/skills`.
4. The project: `<project>/.kagan/skills`.

The same name twice in one place: kagan uses neither, and `kg skill list` shows the error. A user or project name cannot start with `kg-`. That prefix belongs to bundled and plugin skills.

> [!CAUTION]
> Before you start an agent in a repository you do not trust, read `.kagan/skills`. An agent can obey instructions that cause damage to your files.

## Built-in skills

- `kg-cli`: threads, projects, and folders.
- `kg-threads`: create and operate a thread. The note names it before a child thread. A new thread's program receives this skill.
- `kg-plugin-authoring`: make a plugin.
- `kg-plugin-port`: port a bb plugin.
- `kg-skill-creator`: write a skill.
- `kg-ui`: operate the open window from `kg`.
- `kg-plugin-commands`: the `kg` commands that running plugins add. Only when a plugin adds one.

## Where skills come from

kagan reads only its own skill folders: built-in, plugin, your skills in `<data folder>/skills`, and project skills in `.kagan/skills`. It does not read another app's skills.

Each start resolves those. A change gets a new hash, so a running thread keeps the folder it started with. Claude and Codex can also load them with `skill_load`. The other programs do not get that tool.

### Where each program looks

- **Claude** receives the set as a plugin.
- **Cursor** receives it through its per-thread plugin folder.
- **Codex** looks in `.agents/skills`.
- **OpenCode** looks in `.opencode/skills`.
- **Kimi** receives `--skills-dir`. That flag replaces Kimi's own discovery, so kagan also passes Kimi's usual user and project roots.
- **Pi** receives `--skill`.

A program kagan does not recognize, for example `gemini`, `amp`, `aider`, `goose`, `qwen`, `copilot`, `grok`, or `droid`, gets no skills from this path.

### Links kagan adds

kagan adds a link only when the name is free. It leaves your own folders alone and removes only its own links. It never writes a program's global config.

Each link gets its own `.git/info/exclude` line. Those lines stay until you put a real folder at that name. A stale line only hides a name kagan once linked. Your own folder at that name removes the line.

On Windows, if the account cannot create a symlink, Codex or OpenCode starts without those links and the thread shows a warning.

### Folders other programs also read

Codex uses `.agents/skills`. Cursor, Gemini, Kimi, and OpenCode in that repository also read that folder.

A Codex or OpenCode program opened in the same repository outside kagan also sees the links in `.agents/skills` or `.opencode/skills`. kagan has no per-run skills path for those programs that keeps their own skills separate. Two Codex or OpenCode threads in one folder share the links, so a later start repoints them. OpenCode may list a skill twice when a Codex thread linked it under `.agents/skills` and an OpenCode thread linked it under `.opencode/skills`. Kimi, Pi, Claude, and Cursor do not add a repository folder.
