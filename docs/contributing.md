# Contributing

We welcome contributions to Kagan! This page provides quick links to get started.

## Quick Links

| Resource           | Link                                                                             |
| ------------------ | -------------------------------------------------------------------------------- |
| Contributing Guide | [CONTRIBUTING.md](https://github.com/aorumbayev/kagan/blob/main/CONTRIBUTING.md) |
| Issue Tracker      | [GitHub Issues](https://github.com/aorumbayev/kagan/issues)                      |
| Pull Requests      | [GitHub PRs](https://github.com/aorumbayev/kagan/pulls)                          |
| Discussions        | [GitHub Discussions](https://github.com/aorumbayev/kagan/discussions)            |

## Development Setup

```bash
# Clone the repository
git clone https://github.com/aorumbayev/kagan.git
cd kagan

# Install dependencies with uv
uv sync --dev

# Run in dev mode (with hot reload)
uv run poe dev

# Run tests
uv run pytest tests/ -v

# Lint and format (auto-fix + format)
uv run poe fix

# Full check (lint + typecheck + test)
uv run poe check
```

## Code Style

- Python 3.12+ with type annotations
- Ruff for linting and formatting
- CSS in `.tcss` files only (no `DEFAULT_CSS`)
- All keybindings defined in `keybindings.py`

## Testing

```bash
uv run pytest tests/ -v              # All tests (parallel by default)
uv run pytest tests/e2e/test_critical_flows.py  # Single test file
uv run pytest tests/e2e/test_critical_flows.py::TestAutoTaskLifecycle::test_auto_full_lifecycle  # Single test function
uv run pytest -m unit                # Unit tests only
uv run pytest -m integration         # Integration tests
uv run pytest -m e2e                 # End-to-end tests
uv run pytest -m "not slow"          # Exclude slow tests
uv run pytest tests/ -n 0            # Sequential (for debugging)
```

## Docs Preview

```bash
uv run poe docs-serve  # Serve docs at http://127.0.0.1:8000/
uv run poe docs-build  # Build static docs
```

## Workflow Validation

Validate GitHub Actions workflows locally before pushing:

```bash
brew install act         # Install act (macOS)
uv run poe workflows-check  # Validate all workflows
```

See the [CONTRIBUTING.md](https://github.com/aorumbayev/kagan/blob/main/CONTRIBUTING.md) for complete guidelines.
