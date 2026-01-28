```
██╗  ██╗ █████╗  ██████╗  █████╗ ███╗   ██╗
██║ ██╔╝██╔══██╗██╔════╝ ██╔══██╗████╗  ██║
█████╔╝ ███████║██║  ███╗███████║██╔██╗ ██║
██╔═██╗ ██╔══██║██║   ██║██╔══██║██║╚██╗██║
██║  ██╗██║  ██║╚██████╔╝██║  ██║██║ ╚████║
╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═══╝
```

> AI-powered Kanban TUI for autonomous development workflows

[![GitHub release (latest by date)](https://img.shields.io/github/v/release/aorumbayev/kagan)](https://github.com/aorumbayev/kagan/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![PyPI version](https://badge.fury.io/py/kagan.svg)](https://badge.fury.io/py/kagan)

Kagan is a terminal-based Kanban board that integrates AI agents to help you complete development tasks autonomously or collaboratively.

![Kagan Screenshot](https://i.imgur.com/dZRl5V7.png)

## Supported AI CLIs

Available now:

- Claude Code
- OpenCode

Coming soon:

- Gemini
- Codex
- More providers

## Install

### Quick Install (with uv)

```bash
uv tool install kagan
```

### All-in-One Install (includes uv + Python)

```bash
curl -fsSL https://uvget.me/install.sh | bash -s -- kagan
```

> [!NOTE]
> The all-in-one installer automatically installs `uv` and Python if needed, then installs Kagan via `uv tool install`.

## Usage

```bash
kagan                  # Launch TUI
kagan mcp             # Run as MCP server
kagan --help          # Show all options
```

## Documentation

See the [docs/](docs/) folder for detailed documentation:

- [User Guide](docs/user-guide.md) - Full walkthrough of workflows
- [Configuration](docs/config.md) - Agent setup and options
- [Contributing](CONTRIBUTING.md) - Development guidelines

## License

[MIT](LICENSE)

## Star History

<a href="https://www.star-history.com/#aorumbayev/kagan&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=aorumbayev/kagan&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=aorumbayev/kagan&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=aorumbayev/kagan&type=date&legend=top-left" />
 </picture>
</a>
