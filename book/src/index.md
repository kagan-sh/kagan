# kagan

kagan is a desktop application. An agent writes code with you. Each thread operates one program in one terminal and one folder.

1. [Install kagan and start a thread](start.md).
2. [Review the changes](review.md).
3. [Control kagan with `kg`](cli.md).

These words have one meaning here:

- **Project**: a folder that you add. A project is usually a git repository.
- **Thread**: one program in one folder of a project.
- **Program**: the command that a thread operates, for example `claude`, `codex`, or a shell.
- **Plugin**: a folder of JavaScript. A plugin can add parts of the window, commands, or tools for an agent.
- **Skill**: a folder of instructions. An agent can load a skill.
- **Personal**: the project that kagan makes at the first start. Personal is for work that is not in a git repository. You cannot remove Personal.

A program and a plugin operate code that you select. Only you can give approval for the powers of a plugin. Only you can give approval for a request to delete a thread.
