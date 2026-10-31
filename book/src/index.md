# kagan

kagan is a native, self-extending agentic coding IDE for one person on one machine. It is built on Rust and GPUI. It is designed to protect focus and keep the person understanding the code their agents write.

A thread is one program in one folder. A provider is the agent program kagan talks to, such as Claude Code or Codex. A plugin adds a left tool, a center page, an overlay over the terminal, a command, or a tool. A skill is a folder of instructions the agent can load.

- [Get started](start.md) covers build, run, and the first thread.
- [Your data](data.md) explains what kagan keeps and what programs can access.
- [Threads](threads.md) and [the window](window.md) cover daily use.
- [Plugins](plugins.md), [skills](skills.md), and [themes](themes.md) cover what you can add.
- [Build a plugin](build-plugin.md) is the short path. The [spec](spec/architecture.md) is the full description.

> [!IMPORTANT]
> Plugins and agents run code you choose. A plugin's powers are approved only in the app. An agent cannot approve them.
