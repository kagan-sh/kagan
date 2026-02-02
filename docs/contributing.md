# Contributing

We welcome contributions to Kagan! This page provides quick links to get started.

## Quick Links

| Resource | Link |
| -------- | ---- |
| Contributing Guide | [CONTRIBUTING.md](https://github.com/aorumbayev/kagan/blob/main/CONTRIBUTING.md) |
| Issue Tracker | [GitHub Issues](https://github.com/aorumbayev/kagan/issues) |
| Pull Requests | [GitHub PRs](https://github.com/aorumbayev/kagan/pulls) |
| Discussions | [GitHub Discussions](https://github.com/aorumbayev/kagan/discussions) |

## Development Setup

```bash
# Clone the repository
git clone https://github.com/aorumbayev/kagan.git
cd kagan

# Install dependencies with uv
uv sync

# Run in dev mode
uv run poe dev

# Run tests
uv run pytest tests/ -v

# Lint and format
uv run poe fix
```

## Code Style

- Python 3.12+ with type annotations
- Ruff for linting and formatting
- CSS in `.tcss` files only (no `DEFAULT_CSS`)
- All keybindings defined in `keybindings.py`

## Testing

```bash
uv run pytest tests/ -v              # All tests
uv run pytest -m unit                # Unit tests only
uv run pytest -m e2e                 # End-to-end tests
```

See the [CONTRIBUTING.md](https://github.com/aorumbayev/kagan/blob/main/CONTRIBUTING.md) for complete guidelines.
