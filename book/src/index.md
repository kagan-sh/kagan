# kagan

kagan is a desktop IDE for AI assisted software development. You run an agent in a real terminal, review the files it changed, and extend the window with plugins when the built-in parts are not enough.

The window already has threads, a terminal, and the changed files. A plugin adds more of the window: a tool, a page, a command, a skill, or a theme. Nothing from a plugin runs until you approve it.

## The window

From left to right:

- **Rail.** Icons for threads, plugin tools, plugin pages, and Settings.
- **Left sidebar.** Projects and the threads in each project.
- **Center.** The terminal for the thread you selected. You type to the program here.
- **Right sidebar.** Questions, changed files, details, plugin panels, the browser, and extra terminals.

[The window](window.md) describes each part. On a narrow window the sidebars hide; Cmd+B and Cmd+Backslash show them again.

## Start here

1. [Install kagan and start a thread](start.md).
2. [Review the changes](review.md) and send comments back to the program.
3. Drive the same window from a terminal with [`kg`](cli.md), when you want to.
4. [Install a plugin](plugins.md), or [build one](build-plugin.md), when you want the IDE to do something it does not ship with.

## What you will use

- **Threads.** One program in one folder. The program can be Claude Code, Codex, another agent, or a shell. In a git repository the thread gets its own worktree, so the project folder stays as it is. [Threads](threads.md).
- **Review.** The diff of files the thread changed, with comments you paste back into the terminal. [Review changes](review.md).
- **Plugins.** The way you extend kagan. Install one, or write a folder of JavaScript. [Plugins](plugins.md), [Build a plugin](build-plugin.md).
- **Skills.** Instructions an agent can load for one kind of task. [Skills](skills.md).
- **Themes.** `fjord` is the default. You can write your own, and a plugin can ship one. [Themes](themes.md).
- **Your computer.** kagan has no account and sends no telemetry. Update checks stay off until you turn them on. [Your data and privacy](data.md).

Only you can approve a plugin, or approve deleting a thread. An agent cannot do either.

## Words in this guide

- **Project.** A folder you add. A project is usually a git repository.
- **Thread.** One program in one folder of a project.
- **Program.** The command that thread runs, for example `claude`, `codex`, or a shell.
- **Plugin.** A folder of JavaScript. It can add parts of the window, commands, or tools for an agent.
- **Skill.** A folder of instructions an agent can load.
- **Personal.** The project kagan makes at the first start, for work that is not in a git repository. You cannot remove Personal.
