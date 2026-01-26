# Kagan

AI-powered Kanban TUI for autonomous development workflows.

## Quick Start

```bash
uv run kagan
```

## Development

```bash
uv run poe dev          # Dev mode with hot reload
uv run poe check        # Lint + typecheck + test
uv run poe fix          # Auto-fix lint issues
```

## Architecture

- `src/kagan/app.py` - Main application
- `src/kagan/ui/` - Textual screens, widgets, modals
- `src/kagan/database/` - SQLite state management
- `src/kagan/acp/` - Agent Client Protocol implementation
- `src/kagan/agents/` - Agent scheduling and management

## Key Bindings

| Key | Action |
|-----|--------|
| j/k | Navigate up/down |
| h/l | Navigate left/right |
| n | New ticket |
| e | Edit ticket |
| d | Delete ticket |
| [ / ] | Move ticket backward/forward |
| Enter | View ticket details |
| o | Agent output |
| s | Start agent |
| x | Stop agent |
| c | Planner chat |
| q | Quit |
