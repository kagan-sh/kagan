"""FastMCP server setup for Kagan."""

from __future__ import annotations

from pathlib import Path

from mcp.server.fastmcp import FastMCP

from kagan.adapters.db.repositories import TaskRepository
from kagan.agents import planner as planner_models
from kagan.bootstrap import InMemoryEventBus
from kagan.git_utils import has_git_repo
from kagan.mcp.tools import KaganMCPServer
from kagan.paths import get_database_path
from kagan.services.tasks import TaskServiceImpl

_state_manager: TaskServiceImpl | None = None
_server: KaganMCPServer | None = None


async def _get_state_manager() -> TaskServiceImpl:
    """Get or create the global TaskService."""
    global _state_manager
    if _state_manager is None:
        project_root = Path.cwd()
        if not await has_git_repo(project_root):
            raise RuntimeError("Not in a git repository")
        repo = TaskRepository(get_database_path(), project_root=project_root)
        await repo.initialize()
        _state_manager = TaskServiceImpl(repo, InMemoryEventBus())
    return _state_manager


async def _get_server() -> KaganMCPServer:
    """Get or create the MCP server wrapper."""
    global _server
    if _server is None:
        _server = KaganMCPServer(await _get_state_manager())
    return _server


def _create_mcp_server(readonly: bool = False) -> FastMCP:
    """Create FastMCP instance with conditional tool registration."""
    mcp = FastMCP("kagan")

    # Read-only tools (always registered)
    @mcp.tool()
    async def propose_plan(
        tasks: list[planner_models.ProposedTask],
        todos: list[planner_models.ProposedTodo] | None = None,
    ) -> dict:
        """Submit a structured plan proposal for planner mode."""
        proposal = planner_models.PlanProposal.model_validate(
            {"tasks": tasks, "todos": todos or []}
        )
        return {
            "status": "received",
            "task_count": len(proposal.tasks),
            "todo_count": len(proposal.todos),
        }

    @mcp.tool()
    async def get_parallel_tasks(exclude_task_id: str | None = None) -> list[dict]:
        """Get all IN_PROGRESS tasks for coordination.

        Use to discover concurrent work and minimize merge conflicts.
        Pass your own task_id to exclude it from results.
        """
        return await (await _get_server()).get_parallel_tasks(exclude_task_id)

    @mcp.tool()
    async def get_agent_logs(
        task_id: str, log_type: str = "implementation", limit: int = 1
    ) -> list[dict]:
        """Get agent execution logs from any task.

        Use to learn from prior executions and avoid repeating failed approaches.
        Returns most recent N iterations (default 1).
        """
        return await (await _get_server()).get_agent_logs(task_id, log_type, limit)

    # Full-mode tools (PAIR mode only)
    if not readonly:

        @mcp.tool()
        async def get_context(task_id: str) -> dict:
            """Get task context for AI tools."""
            return await (await _get_server()).get_context(task_id)

        @mcp.tool()
        async def update_scratchpad(task_id: str, content: str) -> bool:
            """Append to task scratchpad."""
            return await (await _get_server()).update_scratchpad(task_id, content)

        @mcp.tool()
        async def request_review(task_id: str, summary: str) -> dict:
            """Mark task ready for review. Runs acceptance checks."""
            return await (await _get_server()).request_review(task_id, summary)

    return mcp


def main(readonly: bool = False) -> None:
    """Entry point for kagan-mcp command."""
    global _state_manager, _server
    _state_manager = None  # Reset for fresh instance
    _server = None
    mcp = _create_mcp_server(readonly=readonly)
    mcp.run(transport="stdio")
