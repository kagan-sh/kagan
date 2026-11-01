# kagan

kagan is a desktop app where agents write code with you. Each thread runs one program in its own terminal and folder.

1. [Install kagan and start a thread](start.md).
2. [Review the changes](review.md).
3. [Control kagan with `kg`](cli.md).

These words have one meaning in this book:

- **Project**: a folder that you add, usually a git repository.
- **Thread**: one program that runs in one folder of a project.
- **Program**: the command that a thread runs, such as `claude`, `codex`, or a shell.
- **Plugin**: a folder of JavaScript that adds window parts, commands, or agent tools.
- **Skill**: a folder of instructions that an agent can load.

Personal is a project that kagan makes on first start for work outside a git repository, and you cannot remove it.

Programs and plugins run code that you select. Only you can approve plugin powers. Only you can approve when you delete a thread.
