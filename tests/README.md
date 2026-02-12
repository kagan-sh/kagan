# Test Layout and Rules

Kagan tests are organized by package first, then by test type.

## Directory map

- `tests/core/unit/`: deterministic core logic tests
- `tests/core/smoke/`: minimal core boundary smoke checks
- `tests/mcp/contract/`: MCP tool/resource/schema contract tests
- `tests/mcp/smoke/`: minimal MCP transport smoke checks
- `tests/tui/snapshot/`: Textual visual snapshot regressions
- `tests/tui/smoke/`: critical TUI interaction journeys
- `tests/helpers/`: shared test helpers and fixtures

## Package/type policy

- Core defaults to `unit` tests. Keep these fast, isolated, and deterministic.
- MCP defaults to `contract` tests. Validate schemas, errors, and compatibility behavior.
- TUI defaults to `snapshot` tests for rendering regressions.
- Smoke tests are allowed for critical cross-boundary paths only.

## Authoring standard

- Write one behavior per test. Name tests as behavior specs, not implementation details.
- Use Arrange-Act-Assert with clear spacing; keep setup local to the test unless reuse is obvious.
- Assert externally visible behavior (return values, state transitions, emitted events, rendered output).
- Do not assert implementation internals unless they are part of a stable contract.
- Keep tests deterministic: freeze time/randomness and avoid network/system dependencies.
- Use real adapters only in smoke tests; keep core unit tests free from I/O.
- For MCP tests, assert request/response schema and explicit error semantics.
- For TUI tests, prefer interaction checks (`run_test` + pilot) and keep snapshots focused.
- Avoid tautologies (`assert True`, “call succeeded” without a behavioral assertion).
- Avoid brittle mocks that duplicate production logic; prefer fakes/fixtures at system boundaries.
- Keep helpers small and composable; delete one-off helpers when no longer reused.

## Unit test desiderata (core/unit)

- Isolated: each test builds its own fixture state and can run independently.
- Composable: tests can run in any order and in parallel.
- Deterministic: no wall-clock/random/network dependence without explicit control.
- Specific: failures should point to one behavior and one narrow code path.
- Behavioral: assertions must describe externally visible behavior, not wiring details.
- Structure-insensitive: avoid strict mock choreography that mirrors implementation steps.
- Fast: unit tests should complete quickly and avoid I/O.
- Writable: setup should stay simple; hard-to-write tests indicate interface problems.
- Readable: Arrange-Act-Assert should be obvious at a glance.
- Automated: no manual steps or environment interaction.
- Predictive: failures should strongly indicate real behavior regressions.
- Inspiring: keep signal high; remove tautologies and duplicate table-echo tests.

## Marker enforcement

`tests/conftest.py` auto-applies package and type markers from the file path:

- `tests/core/unit/*` => `@pytest.mark.core` + `@pytest.mark.unit`
- `tests/core/smoke/*` => `@pytest.mark.core` + `@pytest.mark.smoke`
- `tests/mcp/contract/*` => `@pytest.mark.mcp` + `@pytest.mark.contract`
- `tests/mcp/smoke/*` => `@pytest.mark.mcp` + `@pytest.mark.smoke`
- `tests/tui/snapshot/*` => `@pytest.mark.tui` + `@pytest.mark.snapshot`
- `tests/tui/smoke/*` => `@pytest.mark.tui` + `@pytest.mark.smoke`

Do not add these markers manually in test files. Marker ownership is path-based.

Snapshot tests are grouped into one xdist worker for deterministic output.

## Common commands

```bash
uv run pytest tests/core/unit/ -m "core and unit" -v
uv run pytest tests/mcp/contract/ -m "mcp and contract" -v
uv run pytest tests/tui/snapshot/ -m "tui and snapshot" -n 0 -v
uv run pytest tests/tui/smoke/ -m smoke -v
uv run poe check        # quality-gate profile: core reliability + contract + smoke
uv run poe check-full   # full gate + full test suite
uv run poe test-quality
```
