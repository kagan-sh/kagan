# Skills

A skill is a folder of instructions that an agent can load for one kind of task. The folder has a `SKILL.md` file:

```markdown
---
name: release-notes
description: Write release notes from the git log.
---

Read the commits since the last tag. Group them by feature and fix.
```

The lines between the `---` lines give the `name` and the `description`. The name uses lowercase letters and hyphens, and matches the folder name. The rest of the file has the instructions.

## Where skills come from

kagan reads skills from four places. If two places have a skill with the same name, the later place wins:

1. Built into `kg`.
2. An enabled plugin.
3. Your skills: `<data folder>/skills`.
4. The project: `<project>/.kagan/skills`.

If one place has two skills with the same name, kagan uses neither. `kg skill list` shows the error. A user or project skill name cannot start with `kg-`; that prefix belongs to kagan's bundled and plugin skills.

> [!CAUTION]
> Before you start an agent in a repository that you do not trust, read the files in `.kagan/skills`. An agent can obey instructions that cause damage to your files.

The agent sees each skill's name and description. It loads the instructions only when it needs them.

## BB skills in a thread

With agent integration on, a new thread's program also sees the skills in your BB folder and the project's `.bb` folder:

- Your BB skills: `$BB_DATA_DIR/skills`, or `~/.bb/skills` when `BB_DATA_DIR` is unset.
- The project's BB skills: `<thread folder>/.bb/skills`.

Kagan writes them into an immutable per-content catalog beside its resolved skills: built-in, plugin, your Kagan skills, and project skills. A Kagan skill wins a collision with a BB skill, and a BB project skill wins a BB user skill. Each start resolves the catalog; a changed catalog gets a new hash, so a running thread keeps the folder it started with.

Each program kagan recognizes for skill injection gets the catalog where it looks: Claude as a plugin, Cursor through its per-thread plugin folder, Codex under `.codex/skills`, OpenCode under `.opencode/skills`, Kimi with `--skills-dir`, and Pi with `--skill`. Kagan adds a link only when the name is free, leaves your own folders alone, removes only its own links, and gives each link its own `.git/info/exclude` line. It never writes a program's global config, and it does not use `.agents/skills` because Cursor and Gemini read that folder. On Windows, if the account cannot create a symlink, Codex or OpenCode starts without the workspace links and the thread shows a warning.

A Codex or OpenCode session opened in the same repository outside kagan also sees links in `.codex/skills` or `.opencode/skills`; kagan has no per-run skills path for those programs that keeps their own skills. Kimi, Pi, Claude, and Cursor do not add a repository folder.

A program kagan does not recognize (for example `gemini`, `amp`, `aider`, `goose`, `qwen`, `copilot`, `grok`, or `droid`) gets no native skills from this path. Kagan also does not copy BB's built-in or plugin skills: they live in the BB installation and often depend on `bb`.

Your own Kagan skills in `<data folder>/skills` and `.kagan/skills` are part of the same resolved tree, so the program sees them natively too. Claude and Codex can also load them with the `skill_load` tool; the other programs do not get that tool.

## Manage skills

```bash
kg skill list
kg skill show kg-plugin-authoring
kg skill new my-skill
kg skill new my-skill --project
kg skill add <path>
kg skill remove my-skill
```

- `new` makes a folder in your skills. With `--project`, it makes the folder in the project.
- `add` copies a skill folder that you have.
- `remove` deletes one of your skills or a project skill. You cannot remove a built-in skill or a plugin skill.
- A name that starts with `kg-` is reserved; `new` and `add` refuse it.

## Built-in skills

- `kg-cli`: threads, projects, and folders.
- `kg-threads`: create and operate a thread. A new thread's program receives this skill.
- `kg-plugin-authoring`: make a plugin.
- `kg-plugin-port`: port a bb plugin.
- `kg-skill-creator`: write a skill.
- `kg-ui`: operate the open window from `kg`.
