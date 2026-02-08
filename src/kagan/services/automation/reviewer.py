from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.agents.output import serialize_agent_output
from kagan.agents.prompt_loader import get_review_prompt
from kagan.agents.signals import Signal, parse_signal
from kagan.core.models.enums import NotificationSeverity, TaskStatus
from kagan.core.time import utc_now
from kagan.debug_log import log
from kagan.limits import AGENT_TIMEOUT_LONG

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable
    from pathlib import Path

    from kagan.acp import Agent
    from kagan.adapters.db.repositories import ExecutionRepository
    from kagan.adapters.git.operations import GitOperationsProtocol
    from kagan.agents.agent_factory import AgentFactory
    from kagan.config import AgentConfig, KaganConfig
    from kagan.services.runtime import RuntimeService
    from kagan.services.tasks import TaskService
    from kagan.services.types import TaskLike
    from kagan.services.workspaces import WorkspaceService


class AutomationReviewer:
    def __init__(
        self,
        *,
        task_service: TaskService,
        workspace_service: WorkspaceService,
        config: KaganConfig,
        execution_service: ExecutionRepository | None,
        notifier: Callable[[str, str, NotificationSeverity], None] | None,
        agent_factory: AgentFactory,
        git_adapter: GitOperationsProtocol | None,
        runtime_service: RuntimeService,
        get_agent_config: Callable[[TaskLike], AgentConfig],
        apply_model_override: Callable[[Agent, AgentConfig, str], None],
        set_review_agent: Callable[[str, Agent], Awaitable[None]],
        notify_task_changed: Callable[[], None],
    ) -> None:
        self._tasks = task_service
        self._workspaces = workspace_service
        self._config = config
        self._executions = execution_service
        self._notifier = notifier
        self._agent_factory = agent_factory
        self._git = git_adapter
        self._runtime_service = runtime_service

        self._get_agent_config = get_agent_config
        self._apply_model_override = apply_model_override
        self._set_review_agent = set_review_agent
        self._notify_task_changed = notify_task_changed

    def _notify_user(self, message: str, title: str, severity: NotificationSeverity) -> None:
        if self._notifier is not None:
            self._notifier(message, title, severity)

    async def run_review(
        self, task: TaskLike, wt_path: Path, execution_id: str
    ) -> tuple[bool, str]:
        """Run agent-based review and return (passed, summary)."""
        agent_config = self._get_agent_config(task)
        prompt = await self._build_review_prompt(task)

        agent = self._agent_factory(wt_path, agent_config, read_only=True)
        agent.set_auto_approve(True)

        self._apply_model_override(agent, agent_config, f"review of task {task.id}")

        agent.start()

        await self._set_review_agent(task.id, agent)

        try:
            await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
            await agent.send_prompt(prompt)
            response = agent.get_response_text()

            serialized_output = serialize_agent_output(agent)
            if self._executions is not None:
                await self._executions.append_execution_log(execution_id, serialized_output)
                await self._executions.append_agent_turn(
                    execution_id,
                    prompt=prompt,
                    summary=response,
                )

            signal = parse_signal(response)
            if signal.signal == Signal.APPROVE:
                return True, signal.reason
            if signal.signal == Signal.REJECT:
                return False, signal.reason
            return False, "No review signal found in agent response"
        except TimeoutError:
            log.error(f"Review agent timeout for task {task.id}")
            return False, "Review agent timed out"
        except Exception as e:
            log.error(f"Review agent failed for {task.id}: {e}")
            return False, f"Review agent error: {e}"
        finally:
            self._runtime_service.clear_review_agent(task.id)
            await agent.stop()

    async def _handle_complete(self, task: TaskLike) -> None:
        """Handle completion: move to REVIEW then run review if enabled."""
        wt_path = await self._workspaces.get_path(task.id)
        if wt_path is not None and self._git is not None:
            if await self._git.has_uncommitted_changes(str(wt_path)):
                short_id = task.id[:8]
                await self._git.commit_all(
                    str(wt_path),
                    f"chore: adding uncommitted agent changes ({short_id})",
                )
                log.info(f"Auto-committed leftover changes for task {task.id}")

        await self._tasks.update_fields(task.id, status=TaskStatus.REVIEW)
        self._notify_task_changed()

        if not self._config.general.auto_review:
            log.info(f"Auto review disabled, skipping review for task {task.id}")
            return

        wt_path = await self._workspaces.get_path(task.id)
        review_passed = False
        review_note = ""
        review_attempted = False
        execution_id = None
        runtime_view = self._runtime_service.get(task.id)
        if runtime_view is not None:
            execution_id = runtime_view.execution_id

        if wt_path is not None and execution_id is not None:
            review_passed, review_note = await self.run_review(task, wt_path, execution_id)
            review_attempted = True

            status = "approved" if review_passed else "rejected"
            log.info(f"Task {task.id} review: {status}")

            if review_passed:
                self._notify_user(
                    f"✓ Review passed: {task.title[:30]}",
                    title="Review Complete",
                    severity=NotificationSeverity.INFORMATION,
                )
            else:
                self._notify_user(
                    f"✗ Review failed: {review_note[:50]}",
                    title="Review Complete",
                    severity=NotificationSeverity.WARNING,
                )

        if review_note:
            scratchpad = await self._tasks.get_scratchpad(task.id)
            note = f"\n\n--- REVIEW ---\n{review_note}"
            await self._tasks.update_scratchpad(task.id, scratchpad + note)
            self._notify_task_changed()

        if review_attempted and execution_id is not None and self._executions is not None:
            review_result = {
                "status": "approved" if review_passed else "rejected",
                "summary": review_note,
                "completed_at": utc_now().isoformat(),
            }
            await self._executions.update_execution(
                execution_id,
                metadata={"review_result": review_result},
            )

    async def _handle_blocked(self, task: TaskLike, reason: str) -> None:
        """Handle blocked task by moving it back to backlog with context."""
        scratchpad = await self._tasks.get_scratchpad(task.id)
        block_note = f"\n\n--- BLOCKED ---\nReason: {reason}\n"
        await self._tasks.update_scratchpad(task.id, scratchpad + block_note)

        await self._tasks.update_fields(task.id, status=TaskStatus.BACKLOG)
        self._notify_task_changed()

    async def _build_review_prompt(self, task: TaskLike) -> str:
        """Build review prompt from template with commits and diff."""
        base = task.base_branch or self._config.general.default_base_branch
        commits = await self._workspaces.get_commit_log(task.id, base)
        diff_summary = await self._workspaces.get_diff_stats(task.id, base)

        return get_review_prompt(
            title=task.title,
            task_id=task.id,
            description=task.description or "",
            commits="\n".join(f"- {c}" for c in commits) if commits else "No commits",
            diff_summary=diff_summary or "No changes",
        )
