"""Shared fixtures for feature tests."""

from __future__ import annotations

import pytest

from kagan.command_utils import clear_which_cache

# Re-export fixtures from parent conftest so they're available in feature tests
# pytest automatically discovers fixtures from conftest.py files in parent directories


@pytest.fixture(autouse=True)
def _mock_agent_gates_for_ci(monkeypatch: pytest.MonkeyPatch) -> None:
    """Bypass agent-availability and MCP-configuration gates on CI.

    On CI runners the ``claude`` (or other agent) CLI is not installed, so:

    1. ``AgentHealthServiceImpl._check_agent()`` marks the agent as unavailable
       and PlannerScreen never calls ``_start_planner()``.
    2. ``check_agent_installed()`` returns False, blocking PAIR session flows
       behind an AgentChoiceModal.
    3. ``is_global_mcp_configured()`` returns False (no MCP config files on CI),
       causing an ``McpInstallModal`` to block session flows.

    This fixture mocks the two ``shutil.which`` entry-points and the MCP config
    check so that all feature tests run identically on CI and locally.
    """
    clear_which_cache()
    monkeypatch.setattr(
        "kagan.services.agent_health.shutil.which",
        lambda _cmd, *_a, **_kw: "/usr/bin/mock",
    )
    monkeypatch.setattr(
        "kagan.agents.installer.shutil.which",
        lambda _cmd, *_a, **_kw: "/usr/bin/mock",
    )
    monkeypatch.setattr(
        "kagan.mcp.global_config.is_global_mcp_configured",
        lambda _agent: True,
    )
