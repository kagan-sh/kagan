"""Agent and automation test fixtures."""

from __future__ import annotations

import asyncio
from unittest.mock import AsyncMock, MagicMock

import pytest


@pytest.fixture
def mock_workspace_service():
    """Create a mock WorkspaceService."""
    from tests.helpers.mocks import create_mock_workspace_service

    return create_mock_workspace_service()


@pytest.fixture
def mock_agent_spawn(monkeypatch):
    """Mock ACP agent subprocess spawning."""
    original_exec = asyncio.create_subprocess_exec

    async def selective_mock(*args, **kwargs):
        cmd = args[0] if args else ""
        if cmd in ("git", "tmux"):
            return await original_exec(*args, **kwargs)

        mock_process = MagicMock()
        mock_process.pid = 12345
        mock_process.returncode = None
        mock_process.stdout = MagicMock()
        mock_process.stdout.readline = AsyncMock(return_value=b"")
        mock_process.stderr = MagicMock()
        mock_process.stderr.readline = AsyncMock(return_value=b"")
        mock_process.wait = AsyncMock(return_value=0)
        mock_process.terminate = MagicMock()
        mock_process.kill = MagicMock()
        mock_process.communicate = AsyncMock(return_value=(b"", b""))
        return mock_process

    monkeypatch.setattr("asyncio.create_subprocess_exec", selective_mock)


@pytest.fixture
def mock_agent_factory():
    """Factory that returns deterministic mock agents for testing."""
    from tests.helpers.mocks import MockAgentFactory

    return MockAgentFactory()
