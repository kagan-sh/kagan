"""FastMCP server setup for Kagan.

Uses MCP SDK best practices:
- Lifespan for resource management (AppContext, KaganMCPServer)
- Context injection for built-in logging
- Pydantic models for structured responses
- Progress reporting for long operations
"""

from __future__ import annotations

from contextlib import asynccontextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.session import ServerSession

from kagan.agents import planner as planner_models
from kagan.bootstrap import AppContext, create_app_context
from kagan.git_utils import has_git_repo
from kagan.mcp.models import (
    AgentLogEntry,
    LinkedTask,
    PlanProposalResponse,
    RepoInfo,
    ReviewResponse,
    TaskContext,
    TaskDetails,
    TaskSummary,
)
from kagan.mcp.tools import KaganMCPServer
from kagan.mcp_naming import get_mcp_server_name
from kagan.paths import get_config_path, get_database_path

if TYPE_CHECKING:
    from collections.abc import AsyncIterator


@dataclass
class MCPLifespanContext:
    """Context available during MCP server lifetime via lifespan."""

    app_context: AppContext
    server: KaganMCPServer


# Type alias for our Context with lifespan
MCPContext = Context[ServerSession, MCPLifespanContext]


@asynccontextmanager
async def _mcp_lifespan(mcp: FastMCP) -> AsyncIterator[MCPLifespanContext]:
    """Lifespan manager for MCP server resources.

    Creates AppContext and KaganMCPServer on startup, available via ctx.request_context.
    """
    project_root = Path.cwd()
    if not await has_git_repo(project_root):
        raise RuntimeError("Not in a git repository")

    app_ctx = await create_app_context(
        get_config_path(),
        get_database_path(),
        project_root=project_root,
    )
    server = KaganMCPServer(
        app_ctx.task_service,
        workspace_service=app_ctx.workspace_service,
        project_service=app_ctx.project_service,
        execution_service=app_ctx.execution_service,
    )

    yield MCPLifespanContext(app_context=app_ctx, server=server)

    # Cleanup if needed (currently none required)


def _get_server(ctx: MCPContext) -> KaganMCPServer:
    """Extract KaganMCPServer from request context."""
    lifespan_ctx = ctx.request_context.lifespan_context
    if lifespan_ctx is None:
        raise RuntimeError("MCP server not initialized - lifespan context is None")
    return lifespan_ctx.server


def _build_server_instructions(readonly: bool) -> str:
    """Build MCP server instructions tailored to readonly vs full mode."""
    base = [
        "Kagan is a Kanban-style task management system for AI-assisted development.",
        "",
        "The task_id is provided in your system prompt when Kagan assigns you work.",
        "Use get_task to inspect any task (with include_logs=true for execution history).",
        "Use get_parallel_tasks to coordinate with other agents.",
    ]
    if readonly:
        base.extend(
            [
                "",
                "You are running in READ-ONLY mode.",
                "Available tools: propose_plan, get_task, get_parallel_tasks.",
            ]
        )
    else:
        base.extend(
            [
                "",
                "When assigned a task, follow this workflow:",
                "1. Call get_context with your task_id to get requirements and codebase context",
                "2. Use update_scratchpad to record progress, decisions, and blockers",
                "3. Call request_review when implementation is complete",
            ]
        )
    return "\n".join(base)


def _create_mcp_server(readonly: bool = False) -> FastMCP:
    """Create FastMCP instance with lifespan and tools."""
    mcp = FastMCP(
        get_mcp_server_name(),
        instructions=_build_server_instructions(readonly),
        lifespan=_mcp_lifespan,
    )

    @mcp.tool()
    async def propose_plan(
        tasks: list[planner_models.ProposedTask],
        todos: list[planner_models.ProposedTodo] | None = None,
        ctx: MCPContext | None = None,
    ) -> PlanProposalResponse:
        """Submit a structured plan proposal for planner mode."""
        if ctx:
            await ctx.info(f"Receiving plan proposal with {len(tasks)} tasks")

        proposal = planner_models.PlanProposal.model_validate(
            {"tasks": tasks, "todos": todos or []}
        )

        if ctx:
            await ctx.debug(
                f"Plan validated: {len(proposal.tasks)} tasks, {len(proposal.todos)} todos"
            )

        return PlanProposalResponse(
            status="received",
            task_count=len(proposal.tasks),
            todo_count=len(proposal.todos),
            tasks=[task.model_dump(mode="json") for task in proposal.tasks],
            todos=[todo.model_dump(mode="json") for todo in proposal.todos],
        )

    @mcp.tool()
    async def get_parallel_tasks(
        exclude_task_id: str | None = None,
        ctx: MCPContext | None = None,
    ) -> list[TaskSummary]:
        """Get all IN_PROGRESS tasks for coordination."""
        if ctx:
            await ctx.info("Fetching parallel tasks for coordination")

        server = _get_server(ctx) if ctx else None
        if server is None:
            return []

        raw_tasks = await server.get_parallel_tasks(exclude_task_id)

        if ctx:
            await ctx.debug(f"Found {len(raw_tasks)} parallel tasks")

        return [
            TaskSummary(
                task_id=t["task_id"],
                title=t["title"],
                description=t.get("description"),
                scratchpad=t.get("scratchpad"),
            )
            for t in raw_tasks
        ]

    @mcp.tool()
    async def get_task(
        task_id: str,
        include_scratchpad: bool | None = None,
        include_logs: bool | None = None,
        include_review: bool | None = None,
        mode: str = "summary",
        ctx: MCPContext | None = None,
    ) -> TaskDetails:
        """Get task details with optional extended context.

        Args:
            task_id: The task to retrieve
            include_scratchpad: Include agent notes
            include_logs: Include execution logs from previous runs
            include_review: Include review feedback
            mode: 'summary' or 'full'
        """
        if ctx:
            await ctx.info(f"Fetching task details for {task_id}")

        server = _get_server(ctx) if ctx else None
        if server is None:
            raise ValueError("Server not initialized")

        raw = await server.get_task(
            task_id,
            include_scratchpad=include_scratchpad,
            include_logs=include_logs,
            include_review=include_review,
            mode=mode,
        )

        logs = None
        if raw.get("logs"):
            logs = [
                AgentLogEntry(
                    run=log["run"],
                    content=log["content"],
                    created_at=log["created_at"],
                )
                for log in raw["logs"]
            ]

        if ctx:
            await ctx.debug(f"Task retrieved: status={raw['status']}")

        return TaskDetails(
            task_id=raw["task_id"],
            title=raw["title"],
            status=raw["status"],
            description=raw.get("description"),
            acceptance_criteria=raw.get("acceptance_criteria"),
            scratchpad=raw.get("scratchpad"),
            review_feedback=raw.get("review_feedback"),
            logs=logs,
        )

    if not readonly:

        @mcp.tool()
        async def get_context(
            task_id: str,
            ctx: MCPContext | None = None,
        ) -> TaskContext:
            """Get task context for AI tools.

            Returns comprehensive context including task details, workspace info,
            repository state, and linked tasks.
            """
            if ctx:
                await ctx.info(f"Fetching context for task {task_id}")
                await ctx.report_progress(0.1, 1.0, "Loading task details")

            server = _get_server(ctx) if ctx else None
            if server is None:
                raise ValueError("Server not initialized")

            raw = await server.get_context(task_id)

            if ctx:
                await ctx.report_progress(0.5, 1.0, "Processing workspace info")

            # Convert raw repos to RepoInfo models
            repos = [
                RepoInfo(
                    repo_id=r["repo_id"],
                    name=r["name"],
                    path=r["path"],
                    worktree_path=r.get("worktree_path"),
                    target_branch=r.get("target_branch"),
                    has_changes=r.get("has_changes"),
                    diff_stats=r.get("diff_stats"),
                )
                for r in raw.get("repos", [])
            ]

            # Convert linked tasks
            linked_tasks = [
                LinkedTask(
                    task_id=lt["task_id"],
                    title=lt["title"],
                    status=lt["status"],
                    description=lt.get("description"),
                )
                for lt in raw.get("linked_tasks", [])
            ]

            if ctx:
                await ctx.report_progress(1.0, 1.0, "Context ready")
                await ctx.debug(
                    f"Context loaded: {len(repos)} repos, {len(linked_tasks)} linked tasks"
                )

            return TaskContext(
                task_id=raw["task_id"],
                title=raw["title"],
                description=raw.get("description"),
                acceptance_criteria=raw.get("acceptance_criteria"),
                scratchpad=raw.get("scratchpad"),
                workspace_id=raw.get("workspace_id"),
                workspace_branch=raw.get("workspace_branch"),
                workspace_path=raw.get("workspace_path"),
                working_dir=raw.get("working_dir"),
                repos=repos,
                repo_count=raw.get("repo_count", len(repos)),
                linked_tasks=linked_tasks,
            )

        @mcp.tool()
        async def update_scratchpad(
            task_id: str,
            content: str,
            ctx: MCPContext | None = None,
        ) -> bool:
            """Append to task scratchpad.

            Use to record progress, decisions, blockers, and notes during implementation.
            Content is appended to existing scratchpad.
            """
            if ctx:
                await ctx.info(f"Updating scratchpad for task {task_id}")
                await ctx.debug(f"Content length: {len(content)} chars")

            server = _get_server(ctx) if ctx else None
            if server is None:
                raise ValueError("Server not initialized")

            result = await server.update_scratchpad(task_id, content)

            if ctx:
                await ctx.debug("Scratchpad updated successfully")

            return result

        @mcp.tool()
        async def request_review(
            task_id: str,
            summary: str,
            ctx: MCPContext | None = None,
        ) -> ReviewResponse:
            """Mark task ready for review.

            Call this when implementation is complete. The task will move to REVIEW status.
            Include a summary of what was implemented.

            Note: Will fail if there are uncommitted changes - commit your work first.
            """
            if ctx:
                await ctx.info(f"Requesting review for task {task_id}")
                await ctx.report_progress(0.2, 1.0, "Checking for uncommitted changes")

            server = _get_server(ctx) if ctx else None
            if server is None:
                raise ValueError("Server not initialized")

            raw = await server.request_review(task_id, summary)

            if ctx:
                await ctx.report_progress(1.0, 1.0, "Review request complete")
                if raw["status"] == "error":
                    await ctx.warning(f"Review request failed: {raw['message']}")
                else:
                    await ctx.debug("Task moved to REVIEW status")

            return ReviewResponse(
                status=raw["status"],
                message=raw["message"],
            )

    return mcp


def main(readonly: bool = False) -> None:
    """Entry point for kagan-mcp command."""
    mcp = _create_mcp_server(readonly=readonly)
    mcp.run(transport="stdio")
