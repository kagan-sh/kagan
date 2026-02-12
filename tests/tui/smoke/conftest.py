"""Shared fixtures for feature tests."""

from __future__ import annotations

from importlib import import_module

import pytest

from kagan.core.command_utils import clear_which_cache

# Re-export fixtures from parent conftest so they're available in feature tests
# pytest automatically discovers fixtures from conftest.py files in parent directories


@pytest.fixture(autouse=True)
def _mock_agent_gates_for_ci(monkeypatch: pytest.MonkeyPatch) -> None:
    """Bypass agent-availability gates on CI.

    On CI runners the ``claude`` (or other agent) CLI is not installed, so:

    1. ``AgentHealthServiceImpl._check_agent()`` marks the agent as unavailable
       and PlannerScreen never calls ``_start_planner()``.
    2. ``check_agent_installed()`` returns False, blocking PAIR session flows
       behind an AgentChoiceModal.

    This fixture mocks the two ``shutil.which`` entry-points so that all
    feature tests run identically on CI and locally.
    """
    clear_which_cache()
    agent_health_module = import_module("kagan.core.services.agent_health")
    agents_installer_module = import_module("kagan.core.agents.installer")
    monkeypatch.setattr(
        agent_health_module.shutil,
        "which",
        lambda _cmd, *_a, **_kw: "/usr/bin/mock",
    )
    monkeypatch.setattr(
        agents_installer_module.shutil,
        "which",
        lambda _cmd, *_a, **_kw: "/usr/bin/mock",
    )
