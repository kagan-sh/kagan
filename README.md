<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset=".github/assets/logo-light.svg">
    <img alt="Kagan" src=".github/assets/logo-light.svg" width="480">
  </picture>
</p>

<p align="center">
  <strong>AI-powered Kanban TUI for autonomous development workflows</strong>
</p>

<p align="center">
  <a href="https://pypi.org/project/kagan/"><img src="https://img.shields.io/pypi/v/kagan?style=for-the-badge" alt="PyPI"></a>
  <a href="https://pypi.org/project/kagan/"><img src="https://img.shields.io/pypi/pyversions/kagan?style=for-the-badge" alt="Python"></a>
  <a href="https://opensource.org/license/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge" alt="License: MIT"></a>
  <a href="https://github.com/aorumbayev/kagan/stargazers"><img src="https://img.shields.io/github/stars/aorumbayev/kagan?style=for-the-badge" alt="Stars"></a>
  <a href="https://discord.gg/dB5AgMwWMy"><img src="https://img.shields.io/badge/discord-join-5865F2?style=for-the-badge&logo=discord&logoColor=white" alt="Discord"></a>
  <a href="https://snyk.io/test/github/kagan-sh/kagan"><img src="https://img.shields.io/snyk/vulnerabilities/github/kagan-sh/kagan?style=for-the-badge&logo=snyk&logoColor=white" alt="Snyk"></a>
</p>

<p align="center">
  <a href="https://docs.kagan.sh">Documentation</a> •
  <a href="https://docs.kagan.sh/install">Install</a> •
  <a href="https://docs.kagan.sh/quickstart">Quickstart</a> •
  <a href="https://github.com/aorumbayev/kagan/issues">Issues</a>
</p>

---

<p align="center">
  <img src="https://i.imgur.com/dZRl5V7.png" alt="Kagan Screenshot" width="700">
</p>

Terminal Kanban board with integrated AI agents for autonomous and collaborative development. Review mode surfaces merge readiness and conflict guidance.

## Install

=== "UV (Recommended)"

```bash
uv tool install kagan
```

=== "Mac / Linux"

```bash
curl -fsSL https://uvget.me/install.sh | bash -s -- kagan
```

=== "Windows (PowerShell)"

```powershell
iwr -useb uvget.me/install.ps1 -OutFile install.ps1; .\install.ps1 kagan
```

=== "pip"

```bash
pip install kagan
```

### Requirements

- Python 3.12 -- 3.13, Git, terminal 80x20+
- tmux (recommended for PAIR sessions on macOS/Linux)
- VS Code or Cursor (PAIR launchers, especially on Windows)

## Usage

```bash
kagan              # Launch TUI (default command)
kagan tui          # Launch TUI explicitly
kagan core status  # Show status of the core process
kagan core stop    # Stop the running core process
kagan mcp          # Run as MCP server (connects to core via IPC)
kagan tools        # Stateless developer utilities (prompt enhancement)
kagan update       # Check for and install updates
kagan list         # List all projects with task counts
kagan reset        # Reset data (interactive)
kagan --help       # Show all options
```

## Ways To Use Kagan

### TUI-first (interactive)

Run `kagan` -- create tasks, run AUTO/PAIR workflows, review/rebase/merge, switch projects.

### MCP delegation (admin lane)

Operate from your AI CLI while Kagan stays source of truth:

```bash
kagan mcp --identity kagan_admin --capability maintainer --session-id ext:orchestrator
```

Use `viewer`/`pair_worker` profiles for safer day-to-day automation.

## User-Facing Features

- Board lifecycle: `BACKLOG -> IN_PROGRESS -> REVIEW -> DONE`
- Task CRUD, duplicate, inspect details
- Work modes: `AUTO` (background agent), `PAIR` (interactive session)
- Chat-driven planning with approval flow
- Review: diff output, approve/reject/rebase/merge
- Multi-repo: project/repo switching, base-branch controls
- PAIR handoff: session management + human redirect (tmux/VS Code/Cursor)
- MCP: 23 tools spanning tasks, sessions, review, planning, projects, audit, settings
- Core daemon management: run, inspect, stop

## Supported AI CLIs

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) (Anthropic)
- [OpenCode](https://opencode.ai/docs) (SST)
- [Codex](https://github.com/openai/codex) (OpenAI)
- [Gemini CLI](https://github.com/google-gemini/gemini-cli) (Google)
- [Kimi CLI](https://github.com/MoonshotAI/kimi-cli) (Moonshot AI)
- [GitHub Copilot](https://github.com/github/copilot-cli) (GitHub)

## Documentation

Full docs at **[docs.kagan.sh](https://docs.kagan.sh)** -- [User Guide](https://docs.kagan.sh/user-guide/) | [MCP Server](https://docs.kagan.sh/mcp/)

## License

[MIT](LICENSE)

---

<p align="center">
  <a href="https://www.star-history.com/#aorumbayev/kagan&type=date">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=aorumbayev/kagan&type=date&theme=dark" />
      <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=aorumbayev/kagan&type=date" />
      <img alt="Star History" src="https://api.star-history.com/svg?repos=aorumbayev/kagan&type=date" width="600" />
    </picture>
  </a>
</p>
