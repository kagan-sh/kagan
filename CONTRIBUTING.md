# Contributing to Kagan

Thanks for your interest in contributing! This document is the canonical guide for
developers working on the codebase. User documentation lives in `docs/`.

## Prerequisites

- Python 3.12+
- `uv` for dependency management
- A terminal that supports Textual (for running the TUI)

## Getting started

```bash
uv run kagan
```

## Development mode

```bash
uv run poe dev
```

## Linting, formatting, typecheck, tests

```bash
uv run poe fix        # auto-fix lint issues + format
uv run poe lint       # ruff linter
uv run poe typecheck  # pyrefly
uv run pytest tests/ -v
```

Run the full suite:

```bash
uv run poe check
```

## UI snapshots

```bash
UPDATE_SNAPSHOTS=1 uv run pytest tests/test_snapshots.py --snapshot-update
```

## Docs preview

```bash
uv run mkdocs serve
```

Open `http://127.0.0.1:8000/` in your browser.

## Project structure

```
src/kagan/
├── app.py              # Main KaganApp class
├── constants.py        # Shared constants and defaults
├── config.py           # Configuration models
├── database/           # SQLite models + manager
├── styles/             # TCSS styles (single file)
└── ui/                 # Screens, widgets, modals
```

## Notes

- Kagan uses Textual; styles should live in `src/kagan/styles/kagan.tcss`.
- See `AGENTS.md` for agent workflow and coding guidelines.

