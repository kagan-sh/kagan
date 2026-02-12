"""Mock factories for tests.

This package re-exports all public symbols from its submodules so that
existing ``from tests.helpers.mocks import ...`` imports continue to work.
"""

from tests.helpers.mocks.agents import (
    MockAgent,
    MockAgentFactory,
    NoopMessageAgent,
    SmartMockAgent,
    build_repo_routed_smart_agent_factory,
    build_smart_agent_factory,
)
from tests.helpers.mocks.core import (
    create_fake_tmux,
    create_mock_agent,
    create_mock_process,
    create_mock_workspace_service,
    create_test_agent_config,
    create_test_config,
    install_fake_tmux,
)

__all__ = [
    "MockAgent",
    "MockAgentFactory",
    "NoopMessageAgent",
    "SmartMockAgent",
    "build_repo_routed_smart_agent_factory",
    "build_smart_agent_factory",
    "create_fake_tmux",
    "create_mock_agent",
    "create_mock_process",
    "create_mock_workspace_service",
    "create_test_agent_config",
    "create_test_config",
    "install_fake_tmux",
]
