# kagan

kagan is a desktop app for coding with agents. Each thread is a real terminal. The thread runs the agent you already use, such as Claude Code or Codex, in its own folder.

These words have one meaning in this book:

- **Project**: a folder that you add, usually a git repository.
- **Thread**: one program that runs in one folder of a project.
- **Program**: what a thread runs, such as `claude`, `codex`, or a shell.
- **Plugin**: a folder of JavaScript that adds parts to the window, commands, or agent tools.
- **Skill**: a folder of instructions that an agent can load.

## Pages

- [Get started](start.md): install kagan and start your first thread.
- [Threads](threads.md): start, stop, archive, and delete threads.
- [The window](window.md): the sidebars, review, and keys.
- [Your data](data.md): what kagan keeps, and what programs and plugins can do.
- [Plugins](plugins.md), [Skills](skills.md), and [Themes](themes.md): what you can add.
- [Build a plugin](build-plugin.md) and [`kg` command](cli.md): for authors.
- [Terms](CONTEXT.md) and the spec pages: the full description, for contributors.

> [!IMPORTANT]
> Plugins and agents run code that you select. Only a person in the window can approve a plugin and its powers. An agent cannot approve them.
