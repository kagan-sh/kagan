# AGENTS.md - Kagan Coding Agent Guide

Purpose: give coding agents the minimum high-signal rules needed to ship safe, testable changes.

## Rule Scope and Precedence

- This file is the canonical repo-wide agent instruction file.
- `.github/copilot-instructions.md` defers to this file.
- No Cursor rules are currently present (`.cursorrules` and `.cursor/rules/` absent).
- Keep this file concise and action-oriented; avoid architecture essays and ticket-era process detail.
- Keep entries short, self-contained, and broadly applicable.
- Avoid conflicting instructions across files.
- Put file-type or directory-specific rules in `.github/instructions/*.instructions.md` if needed.

## Engineering Principles (PEP 20, Pragmatic Form)

- Prefer explicit over implicit behavior.
- Prefer simple, readable solutions over clever ones.
- Avoid ambiguity; define assumptions in code/tests.
- Fail loudly on invalid state unless failure is intentionally suppressed.
- Keep one obvious way to do routine tasks (build, test, lint, release).

## Instruction Maintenance

- Treat this file as operational policy, not a design document.
- Keep critical commands and rules near the top.
- Remove stale migration-era instructions once they stop affecting behavior.
- Prefer additive, scoped updates over large rewrites.

## Setup

```bash
uv sync --dev
uv run kagan
uv run poe dev
```

## Build / Lint / Format / Typecheck

```bash
uv build --wheel --package kagan
uv run poe lint
uv run poe format
uv run poe fix
uv run poe typecheck
uv run poe check
uv run pre-commit run --all-files
```

## Test Commands (Use These First)

```bash
# all tests
uv run poe test
uv run pytest tests/ -v

# single file/class/test
uv run pytest tests/core/unit/test_runtime_state_service.py -v
uv run pytest tests/core/unit/test_runtime_state_service.py::TestRuntimeStateService -v
uv run pytest tests/mcp/contract/test_mcp_v2_end_to_end.py::test_end_to_end_job_flow_uses_submit_wait_events_contract -v

# filter
uv run pytest tests/ -k "runtime and refresh" -v
uv run pytest tests/ -m "core and unit" -v

# sequential (debug/snapshot safety)
uv run pytest tests/ -n 0 -v
```

## High-Signal Test Profiles

```bash
uv run poe test-core
uv run poe test-mcp
uv run poe test-smoke
uv run poe test-tui-snapshot
uv run poe test-snapshot-update
```

## Pytest and Marker Policy

- Default addopts include `-n auto --dist=loadgroup`.
- Snapshot tests must run with `-n 0`.
- Package/type markers are path-assigned in `tests/conftest.py`.
- Do not manually add: `core`, `mcp`, `tui`, `unit`, `contract`, `snapshot`, `smoke`.
- `integration` marker is deprecated/disallowed for explicit use.

Path mapping:

- `tests/core/unit/*` -> `core`, `unit`
- `tests/core/smoke/*` -> `core`, `smoke`
- `tests/mcp/contract/*` -> `mcp`, `contract`
- `tests/mcp/smoke/*` -> `mcp`, `smoke`
- `tests/tui/snapshot/*` -> `tui`, `snapshot`
- `tests/tui/smoke/*` -> `tui`, `smoke`

## Code Style

- Ruff is source of truth (`line-length = 100`, target `py312`).
- Use `from __future__ import annotations` in Python modules.
- Import order:
  - standard library
  - third-party
  - local `kagan.*`
  - type-only imports under `if TYPE_CHECKING:`
- Type annotate public APIs and meaningful internal boundaries.
- Prefer `X | None` over `Optional[X]`.
- Use `Protocol` for service interfaces/boundaries.
- Naming:
  - classes: `PascalCase`
  - functions/variables: `snake_case`
  - constants: `UPPER_SNAKE_CASE`
  - private members: `_leading_underscore`

## Error Handling and Safety

- Never use bare `except:`.
- Catch specific exceptions when possible.
- Use `ValueError` for invalid input/data-contract violations.
- Use `RuntimeError` for runtime/environment failures.
- Preserve causal chain with `raise ... from exc`.
- Broad `except Exception` only at explicit resilience boundaries.
- In critical core files, broad exceptions require `quality-allow-broad-except`.
- In critical core files, unbounded queues require `quality-allow-unbounded-queue`.

## Stable Architecture Boundaries

- Core daemon owns mutable runtime state.
- SQLite is the single persisted source of truth.
- DB writes happen through core services/adapters only.
- TUI, MCP, and CLI are frontends over core operations.
- No client-side mutation fallback around core.
- Direct subprocess calls are only allowed in:
  - `src/kagan/core/adapters/process.py`

## TUI Rules

- Keep Textual styling in:
  - `src/kagan/tui/styles/kagan.tcss`
- Do not add `DEFAULT_CSS` in Python widgets/screens.
- Reuse shared keybindings from:
  - `src/kagan/tui/keybindings.py`

## Docs and Workflow Validation

```bash
uv run poe docs-serve
uv run poe docs-build
uv run poe workflows-check
```

## Commit Conventions

- Allowed tags:
  - `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `style`, `refactor`, `test`
- Keep commits small and single-purpose.
- In CI/automation contexts, disable GPG signing if needed:
  - `git config commit.gpgsign false`
- Before push:
  - `uv run poe fix`
  - `uv run poe check`

## Done Checklist for Agents

- Ran the smallest relevant tests first, then broader gates as needed.
- Do not claim completion without executing verification commands.
- Updated tests/docs when behavior or contracts changed.
- Kept changes within existing boundaries unless task explicitly required boundary changes.
- Left a short summary with changed files and verification commands executed.
