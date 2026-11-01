# Skills

A skill is a folder of instructions that an agent can load for one kind of task. The folder has a `SKILL.md` file:

```markdown
---
name: release-notes
description: Write release notes from the git log.
---

Read the commits since the last tag. Group them by feature and fix.
```

The lines between the `---` lines give the `name` and the `description`. The name uses lowercase letters and hyphens, and it is the same as the folder name. The rest of the file is the instructions.

## Where skills come from

kagan reads skills from four places. If two places have a skill with the same name, the later place wins:

1. Built into `kg`.
2. An enabled plugin.
3. Your skills: `<data folder>/skills`.
4. The project: `<project>/.kagan/skills`.

If one place has two skills with the same name, kagan uses neither. `kg skill list` shows the error.

> [!WARNING]
> A repository that you clone can have skills in `.kagan/skills`. Your agent can load them and obey them. Read them before you start an agent in a repository that you do not trust.

## How an agent uses skills

The agent sees the name and the description of each skill. It loads the instructions only when it needs them.

## Commands

```bash
kg skill list
kg skill show kg-plugin-authoring
kg skill new my-skill
kg skill new my-skill --project
kg skill add <path>
kg skill remove my-skill
```

- `new` makes a skill folder in your skills. With `--project`, it makes the folder in the project.
- `add` copies a skill folder that you have.
- `remove` deletes one of your skills or a project skill. You cannot remove a built-in skill or a plugin skill.

## Built-in skills

- `kg-cli`: threads, projects, and folders.
- `kg-plugin-authoring`: make a plugin.
- `kg-skill-creator`: write a skill.
- `kg-ui`: operate the open window from `kg`.
