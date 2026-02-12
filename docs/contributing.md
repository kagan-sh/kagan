---
title: Contributing
description: How to contribute to Kagan
icon: material/heart
---

# Contributing

| Resource           | Link                                                                             |
| ------------------ | -------------------------------------------------------------------------------- |
| Contributing Guide | [CONTRIBUTING.md](https://github.com/aorumbayev/kagan/blob/main/CONTRIBUTING.md) |
| Issue Tracker      | [GitHub Issues](https://github.com/aorumbayev/kagan/issues)                      |
| Pull Requests      | [GitHub PRs](https://github.com/aorumbayev/kagan/pulls)                          |
| Discussions        | [GitHub Discussions](https://github.com/aorumbayev/kagan/discussions)            |

## Prerequisites

- Python 3.12 -- 3.13
- `uv` for dependency management
- Git (for worktree functionality)
- tmux (for PAIR mode testing on macOS/Linux)

## Development setup

```bash
git clone https://github.com/aorumbayev/kagan.git
cd kagan
uv sync --dev                   # Install dependencies
uv run poe dev                  # Dev mode with hot reload
uv run poe install-local        # Install as local CLI tool
uv run pytest tests/ -v         # Run tests
uv run poe fix                  # Lint + format
uv run poe check                # Lint + typecheck + test
```

## Architecture

Contributor architecture: [Architecture](reference/architecture.md) (package boundaries, runtime contracts, dispatch authority).

## Packaging

Kagan is a single package (`src/kagan/`) published to PyPI as `kagan`. Built with `uv build --wheel`.

## Code style

See [AGENTS.md](https://github.com/aorumbayev/kagan/blob/main/AGENTS.md#code-style).

## Testing

See [AGENTS.md](https://github.com/aorumbayev/kagan/blob/main/AGENTS.md#running-tests).

## Docs preview

```bash
uv run poe docs-serve  # http://127.0.0.1:8000/
uv run poe docs-build
```

## Workflow validation

```bash
brew install act                    # macOS
uv run poe workflows-check         # Validate all workflows
```

Full guidelines: [CONTRIBUTING.md](https://github.com/aorumbayev/kagan/blob/main/CONTRIBUTING.md).
