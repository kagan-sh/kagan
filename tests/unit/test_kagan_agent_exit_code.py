"""Tests for KaganAgent process exit semantics."""

from pathlib import Path

import pytest
from tests.helpers.mocks import create_test_agent_config

from kagan.acp.kagan_agent import KaganAgent


def _build_agent() -> KaganAgent:
    return KaganAgent(Path("."), create_test_agent_config())


@pytest.mark.parametrize("field_name", ["_stop_requested", "_prompt_completed"])
def test_sigterm_is_ignored_after_shutdown_signal(field_name: str) -> None:
    agent = _build_agent()
    setattr(agent, field_name, True)
    assert agent._should_ignore_exit_code(-15)


def test_non_sigterm_exit_is_not_ignored() -> None:
    agent = _build_agent()
    agent._stop_requested = True
    assert not agent._should_ignore_exit_code(1)
