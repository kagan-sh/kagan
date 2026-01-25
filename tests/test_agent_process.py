"""Tests for AgentProcess and AgentManager."""

from __future__ import annotations

import asyncio
import tempfile
from pathlib import Path

import pytest

from kagan.agents import AgentManager, AgentProcess, AgentState


@pytest.fixture
def temp_dir():
    with tempfile.TemporaryDirectory() as tmpdir:
        yield Path(tmpdir)


class TestAgentProcess:
    async def test_spawn_simple_command(self, temp_dir: Path) -> None:
        agent = AgentProcess("test-001")
        await agent.start("echo hello", cwd=temp_dir)
        await agent.wait_for_exit()
        output, _ = agent.get_output()
        assert "hello" in output
        assert agent.state == AgentState.FINISHED
        assert agent.return_code == 0

    async def test_spawn_in_cwd(self, temp_dir: Path) -> None:
        (temp_dir / "marker.txt").write_text("found")
        agent = AgentProcess("test-002")
        await agent.start("cat marker.txt", cwd=temp_dir)
        await agent.wait_for_exit()
        output, _ = agent.get_output()
        assert "found" in output

    async def test_terminate(self, temp_dir: Path) -> None:
        agent = AgentProcess("test-004")
        await agent.start("sleep 60", cwd=temp_dir)
        await asyncio.sleep(0.1)
        await agent.terminate()
        assert agent.state in (AgentState.FINISHED, AgentState.FAILED)

    async def test_output_truncation(self, temp_dir: Path) -> None:
        agent = AgentProcess("test-005")
        await agent.start("seq 1 10000", cwd=temp_dir)
        await agent.wait_for_exit()
        output, _truncated = agent.get_output(limit=1000)
        assert len(output) <= 1100  # Allow some margin

    async def test_env_vars(self, temp_dir: Path) -> None:
        agent = AgentProcess("test-008")
        await agent.start("echo $TEST_VAR", cwd=temp_dir, env={"TEST_VAR": "myvalue"})
        await agent.wait_for_exit()
        output, _ = agent.get_output()
        assert "myvalue" in output


class TestAgentManager:
    async def test_spawn_multiple(self, temp_dir: Path) -> None:
        manager = AgentManager()
        await manager.spawn("t1", "sleep 60", temp_dir)
        await manager.spawn("t2", "sleep 60", temp_dir)
        assert len(manager.list_active()) == 2
        await manager.terminate_all()
        assert len(manager.list_active()) == 0

    async def test_duplicate_spawn_raises(self, temp_dir: Path) -> None:
        manager = AgentManager()
        await manager.spawn("t1", "sleep 60", temp_dir)
        with pytest.raises(ValueError, match="already running"):
            await manager.spawn("t1", "sleep 60", temp_dir)
        await manager.terminate_all()

    async def test_get_agent(self, temp_dir: Path) -> None:
        manager = AgentManager()
        agent = await manager.spawn("t1", "echo test", temp_dir)
        assert manager.get("t1") is agent
        assert manager.get("nonexistent") is None
        await manager.terminate_all()

    async def test_terminate_single(self, temp_dir: Path) -> None:
        manager = AgentManager()
        await manager.spawn("t1", "sleep 60", temp_dir)
        await manager.spawn("t2", "sleep 60", temp_dir)
        await manager.terminate("t1")
        assert manager.get("t1") is None
        assert manager.get("t2") is not None
        await manager.terminate_all()
