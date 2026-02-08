from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, cast

import pytest
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.mocks import create_fake_tmux
from tests.helpers.wait import wait_for_modal, wait_for_screen, wait_for_widget
from textual.widgets import TabbedContent

from kagan.acp import messages
from kagan.adapters.db.repositories import RepoRepository, TaskRepository
from kagan.adapters.db.schema import Task
from kagan.app import KaganApp
from kagan.core.events import AutomationAgentAttached, AutomationReviewAgentAttached
from kagan.core.models.enums import ExecutionStatus, TaskPriority, TaskStatus, TaskType
from kagan.services.sessions import SessionServiceImpl
from kagan.ui.modals.agent_output import AgentOutputModal
from kagan.ui.modals.confirm import ConfirmModal
from kagan.ui.modals.review import ReviewModal
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.widgets.card import TaskCard
from kagan.ui.widgets.chat_panel import ChatPanel

if TYPE_CHECKING:
    from pathlib import Path

    from textual.pilot import Pilot


@dataclass(frozen=True)
class TaskIds:
    auto_backlog: str
    auto_in_progress: str
    pair_in_progress: str
    pair_review: str
    auto_review: str
    auto_done: str


async def _seed_app(tmp_path: Path) -> tuple[KaganApp, TaskIds]:
    repo_path = tmp_path / "repo"
    repo_path.mkdir()
    await init_git_repo_with_commit(repo_path)

    config_path = tmp_path / "config.toml"
    config_path.write_text(
        """[general]
auto_review = true
default_base_branch = "main"
default_worker_agent = "claude"
default_pair_terminal_backend = "tmux"

[ui]
skip_pair_instructions = true

[agents.claude]
identity = "claude.ai"
name = "Claude"
short_name = "claude"
run_command."*" = "echo mock-claude"
interactive_command."*" = "echo mock-claude-interactive"
active = true
""",
        encoding="utf-8",
    )

    db_path = tmp_path / "kagan.db"
    manager = TaskRepository(db_path, project_root=repo_path)
    await manager.initialize()
    project_id = await manager.ensure_test_project("Enter Flow")

    assert manager._session_factory is not None
    repo_repo = RepoRepository(manager._session_factory)
    repo, _ = await repo_repo.get_or_create(repo_path, default_branch="main")
    if repo.id:
        await repo_repo.add_to_project(project_id, repo.id, is_primary=True)

    tasks = [
        Task(
            id="auto0001",
            project_id=project_id,
            title="AUTO backlog",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.BACKLOG,
            task_type=TaskType.AUTO,
        ),
        Task(
            id="auto0002",
            project_id=project_id,
            title="AUTO in progress",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.IN_PROGRESS,
            task_type=TaskType.AUTO,
        ),
        Task(
            id="pair0001",
            project_id=project_id,
            title="PAIR in progress",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.IN_PROGRESS,
            task_type=TaskType.PAIR,
        ),
        Task(
            id="prvw0001",
            project_id=project_id,
            title="PAIR review",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.REVIEW,
            task_type=TaskType.PAIR,
        ),
        Task(
            id="revw0001",
            project_id=project_id,
            title="AUTO review",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.REVIEW,
            task_type=TaskType.AUTO,
        ),
        Task(
            id="done0001",
            project_id=project_id,
            title="AUTO done",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.DONE,
            task_type=TaskType.AUTO,
        ),
    ]
    for task in tasks:
        await manager.create(task)
    await manager.close()

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=repo_path,
    )
    return (
        app,
        TaskIds(
            auto_backlog="auto0001",
            auto_in_progress="auto0002",
            pair_in_progress="pair0001",
            pair_review="prvw0001",
            auto_review="revw0001",
            auto_done="done0001",
        ),
    )


async def _focus_task(pilot: Pilot, task_id: str) -> None:
    await wait_for_widget(pilot, f"#card-{task_id}", timeout=10.0)
    card = pilot.app.screen.query_one(f"#card-{task_id}", TaskCard)
    card.focus()
    await pilot.pause()


def _mark_auto_runtime_running(app: KaganApp, task_id: str, agent: Any | None = None) -> None:
    runtime = app.ctx.runtime_service
    runtime.mark_started(task_id)
    runtime.set_execution(task_id, None, 0)
    if agent is not None:
        runtime.attach_running_agent(task_id, cast("Any", agent))


@pytest.mark.asyncio
async def test_enter_auto_backlog_prompts_before_start(tmp_path: Path) -> None:
    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        await _focus_task(pilot, task_ids.auto_backlog)
        await pilot.press("enter")
        await wait_for_modal(pilot, ConfirmModal, timeout=5.0)


@pytest.mark.asyncio
async def test_enter_pair_in_progress_prompts_before_attach(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    async def _fast_attach(self: SessionServiceImpl, session_name: str) -> bool:
        del self, session_name
        return True

    monkeypatch.setattr(SessionServiceImpl, "_attach_tmux_session", _fast_attach)

    # Mock tmux + terminal installer so the PAIR flow works on all platforms
    sessions: dict[str, dict[str, Any]] = {}
    fake_tmux = create_fake_tmux(sessions)
    monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
    monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)
    monkeypatch.setattr(
        "kagan.terminals.installer.shutil.which",
        lambda _cmd, *_a, **_kw: "/usr/bin/mock",
    )

    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        await _focus_task(pilot, task_ids.pair_in_progress)
        await pilot.press("enter")
        await wait_for_modal(pilot, ConfirmModal, timeout=5.0)


@pytest.mark.asyncio
async def test_enter_auto_in_progress_opens_review_workspace_output_tab(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    class _DummyAgent:
        def set_message_target(self, _target) -> None:
            return None

    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)
        _mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress, agent=_DummyAgent())
        await _focus_task(pilot, task_ids.auto_in_progress)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)
        tabs = review.query_one("#review-tabs", TabbedContent)
        assert tabs.active == "review-agent-output"
        assert not review.query_one("#review-agent-output-chat").has_class("queue-disabled")
        review.query_one("#review-agent-output-chat .chat-input")


@pytest.mark.asyncio
async def test_enter_auto_in_progress_attaches_live_stream_immediately_from_waited_agent(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    class _DummyAgent:
        def set_message_target(self, _target) -> None:
            return None

    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service
        _mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        agent = _DummyAgent()

        async def _wait_for_running_agent(task_id: str, **_kwargs) -> Any:
            del _kwargs
            if task_id == task_ids.auto_in_progress:
                return agent
            return None

        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        await _focus_task(pilot, task_ids.auto_in_progress)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)

        rendered = ""
        for _ in range(40):
            await pilot.pause()
            rendered = review.query_one(
                "#review-agent-output-chat", ChatPanel
            ).output.get_text_content()
            if "Connected to live agent stream" in rendered:
                break
            await asyncio.sleep(0.05)

        assert "Connected to live agent stream" in rendered


@pytest.mark.asyncio
async def test_enter_auto_in_progress_attaches_stream_when_agent_appears_after_modal_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    class _DummyAgent:
        def set_message_target(self, _target) -> None:
            return None

    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)

        state: dict[str, Any] = {"agent": None}
        _mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        await _focus_task(pilot, task_ids.auto_in_progress)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)

        state["agent"] = _DummyAgent()
        kagan_app.ctx.runtime_service.attach_running_agent(
            task_ids.auto_in_progress,
            cast("Any", state["agent"]),
        )
        await kagan_app.ctx.event_bus.publish(
            AutomationAgentAttached(task_id=task_ids.auto_in_progress)
        )

        connected = False
        for _ in range(40):
            await pilot.pause()
            rendered = review.query_one(
                "#review-agent-output-chat", ChatPanel
            ).output.get_text_content()
            if "Connected to live agent stream" in rendered:
                connected = True
                break
            await asyncio.sleep(0.05)
        assert connected


@pytest.mark.asyncio
async def test_enter_auto_in_progress_waiting_mode_shows_informative_output(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)
        _mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        await _focus_task(pilot, task_ids.auto_in_progress)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)

        rendered = ""
        for _ in range(40):
            await pilot.pause()
            rendered = review.query_one(
                "#review-agent-output-chat", ChatPanel
            ).output.get_text_content()
            if "Waiting for live agent stream..." in rendered:
                break
            await asyncio.sleep(0.05)

        assert "Waiting for live agent stream..." in rendered
        assert rendered.strip() != ""


@pytest.mark.asyncio
async def test_enter_auto_in_progress_backfill_with_empty_payload_shows_fallback_note(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    class _Execution:
        id = "exec-empty"
        status = ExecutionStatus.COMPLETED
        metadata_ = {}

    class _Entry:
        logs = '{"messages":[]}'

    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)
        execution_service = kagan_app.ctx.execution_service

        async def _get_latest_execution_for_task(_task_id: str) -> Any:
            return _Execution()

        async def _get_execution_log_entries(_execution_id: str) -> list[Any]:
            return [_Entry()]

        async def _get_execution(_execution_id: str) -> Any:
            return _Execution()

        monkeypatch.setattr(
            execution_service,
            "get_latest_execution_for_task",
            _get_latest_execution_for_task,
        )
        monkeypatch.setattr(
            execution_service,
            "get_execution_log_entries",
            _get_execution_log_entries,
        )
        monkeypatch.setattr(
            execution_service,
            "get_execution",
            _get_execution,
        )

        await _focus_task(pilot, task_ids.auto_in_progress)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)

        rendered = ""
        for _ in range(40):
            await pilot.pause()
            rendered = review.query_one(
                "#review-agent-output-chat", ChatPanel
            ).output.get_text_content()
            if "contains no displayable output yet" in rendered:
                break
            await asyncio.sleep(0.05)

        assert "contains no displayable output yet" in rendered
        assert rendered.strip() != ""


@pytest.mark.asyncio
async def test_enter_auto_in_progress_recovers_stale_running_execution_without_logs(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    class _StaleExecution:
        def __init__(self, id: str, status: ExecutionStatus) -> None:
            self.id = id
            self.status = status

    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service
        execution_service = kagan_app.ctx.execution_service

        state: dict[str, Any] = {"running": False, "agent": None}

        async def _wait_for_running_agent(task_id: str, **_kwargs) -> Any:
            del _kwargs
            if task_id != task_ids.auto_in_progress:
                return None
            return state["agent"]

        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        async def _spawn_for_task(task) -> bool:
            if task.id == task_ids.auto_in_progress:
                state["running"] = True
                kagan_app.ctx.runtime_service.mark_started(task_ids.auto_in_progress)
                kagan_app.ctx.runtime_service.set_execution(task_ids.auto_in_progress, None, 0)
                return True
            return False

        monkeypatch.setattr(automation, "spawn_for_task", _spawn_for_task)

        async def _get_latest_execution_for_task(task_id: str):
            if task_id == task_ids.auto_in_progress:
                return _StaleExecution(id="stale0001", status=ExecutionStatus.RUNNING)
            return None

        async def _get_execution_log_entries(_execution_id: str):
            return []

        async def _update_execution(execution_id: str, **_kwargs):
            del execution_id, _kwargs
            return None

        monkeypatch.setattr(
            execution_service,
            "get_latest_execution_for_task",
            _get_latest_execution_for_task,
        )
        monkeypatch.setattr(
            execution_service,
            "get_execution_log_entries",
            _get_execution_log_entries,
        )
        monkeypatch.setattr(execution_service, "update_execution", _update_execution)

        await _focus_task(pilot, task_ids.auto_in_progress)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)

        rendered = ""
        for _ in range(40):
            await pilot.pause()
            rendered = review.query_one(
                "#review-agent-output-chat", ChatPanel
            ).output.get_text_content()
            if (
                "Connected to live agent stream" in rendered
                or "Waiting for live agent stream..." in rendered
            ):
                break
            await asyncio.sleep(0.05)
        assert rendered.strip() != ""
        assert (
            "Connected to live agent stream" in rendered
            or "Waiting for live agent stream..." in rendered
        )
        assert state["running"] is True


@pytest.mark.asyncio
async def test_enter_auto_review_attaches_live_review_stream_when_agent_appears_late(
    tmp_path: Path,
) -> None:
    class _DummyAgent:
        def set_message_target(self, _target) -> None:
            return None

    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)

        await _focus_task(pilot, task_ids.auto_review)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)

        kagan_app.ctx.runtime_service.attach_review_agent(
            task_ids.auto_review,
            cast("Any", _DummyAgent()),
        )
        await kagan_app.ctx.event_bus.publish(
            AutomationReviewAgentAttached(task_id=task_ids.auto_review)
        )

        connected = False
        for _ in range(20):
            await pilot.pause()
            rendered = review.query_one("#ai-review-chat", ChatPanel).output.get_text_content()
            if "Connected to live review stream" in rendered:
                connected = True
                break
            await asyncio.sleep(0.02)
        assert connected


@pytest.mark.asyncio
async def test_enter_auto_backlog_backfills_history_when_live_agent_unavailable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service

        async def _spawn_for_task(_task) -> bool:
            kagan_app.ctx.runtime_service.mark_started(task_ids.auto_backlog)
            kagan_app.ctx.runtime_service.set_execution(task_ids.auto_backlog, "exec-backfill", 1)
            return True

        async def _wait_for_running_agent(task_id: str, **_kwargs) -> Any:
            del task_id, _kwargs
            return None

        monkeypatch.setattr(automation, "spawn_for_task", _spawn_for_task)
        monkeypatch.setattr(
            automation,
            "is_running",
            lambda task_id: task_id == task_ids.auto_backlog,
        )
        monkeypatch.setattr(automation, "get_running_agent", lambda _task_id: None)
        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        class _Entry:
            logs = '{"messages":[{"type":"response","content":"history backfill line"}]}'

        class _Execution:
            id = "exec-backfill"

        async def _get_execution_log_entries(_execution_id: str) -> list[Any]:
            return [_Entry()]

        async def _get_execution(_execution_id: str) -> Any:
            return None

        async def _get_latest_execution_for_task(_task_id: str) -> Any:
            return _Execution()

        monkeypatch.setattr(
            kagan_app.ctx.execution_service,
            "get_execution_log_entries",
            _get_execution_log_entries,
        )
        monkeypatch.setattr(kagan_app.ctx.execution_service, "get_execution", _get_execution)
        monkeypatch.setattr(
            kagan_app.ctx.execution_service,
            "get_latest_execution_for_task",
            _get_latest_execution_for_task,
        )

        await _focus_task(pilot, task_ids.auto_backlog)
        await pilot.press("enter")
        await wait_for_modal(pilot, ConfirmModal, timeout=5.0)
        await pilot.press("enter")

        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)

        rendered = ""
        for _ in range(30):
            await pilot.pause()
            rendered = review.query_one(
                "#review-agent-output-chat", ChatPanel
            ).output.get_text_content()
            if "history backfill line" in rendered:
                break
            await asyncio.sleep(0.05)
        assert "history backfill line" in rendered


@pytest.mark.asyncio
async def test_enter_auto_modal_refreshes_when_task_moves_to_review(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    class _DummyAgent:
        def set_message_target(self, _target) -> None:
            return None

    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)
        _mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress, agent=_DummyAgent())

        await _focus_task(pilot, task_ids.auto_in_progress)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)
        tabs = review.query_one("#review-tabs", TabbedContent)
        assert tabs.active == "review-agent-output"

        await kagan_app.ctx.task_service.update_fields(
            task_ids.auto_in_progress,
            status=TaskStatus.REVIEW,
        )

        for _ in range(50):
            await pilot.pause()
            if tabs.active == "review-ai":
                break
            await asyncio.sleep(0.05)

        assert tabs.active == "review-ai"
        assert not review.query_one(".button-row").has_class("hidden")


@pytest.mark.asyncio
async def test_review_modal_sigterm_stream_end_is_not_rendered_as_error(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    class _DummyAgent:
        def set_message_target(self, _target) -> None:
            return None

    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)
        _mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress, agent=_DummyAgent())

        await _focus_task(pilot, task_ids.auto_in_progress)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)
        review.post_message(messages.AgentFail("Agent exited with code -15"))
        await pilot.pause()
        await pilot.pause()

        review_text = review.query_one("#ai-review-chat", ChatPanel).output.get_text_content()
        agent_output_text = review.query_one(
            "#review-agent-output-chat", ChatPanel
        ).output.get_text_content()
        rendered = f"{review_text}\n{agent_output_text}"
        assert "Agent stream ended by cancellation (SIGTERM)." in rendered
        assert "Error: Agent exited with code -15" not in rendered


@pytest.mark.asyncio
async def test_agent_output_modal_sigterm_stream_end_is_not_rendered_as_error(
    tmp_path: Path,
) -> None:
    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        kagan_app = cast("KaganApp", pilot.app)
        task = await kagan_app.ctx.task_service.get_task(task_ids.auto_in_progress)
        assert task is not None

        await pilot.app.push_screen(
            AgentOutputModal(
                task=task,
                agent=None,
                execution_id=None,
                run_count=1,
                is_running=False,
            )
        )
        modal = await wait_for_screen(pilot, AgentOutputModal, timeout=10.0)
        modal.post_message(messages.AgentFail("Agent exited with code -15"))
        await pilot.pause()
        await pilot.pause()

        rendered = modal.query_one("#agent-chat", ChatPanel).output.get_text_content()
        assert "Agent stream ended by cancellation (SIGTERM)." in rendered
        assert "Error: Agent exited with code -15" not in rendered


@pytest.mark.asyncio
async def test_enter_review_opens_review_modal_with_agent_output_tab(tmp_path: Path) -> None:
    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        await _focus_task(pilot, task_ids.auto_review)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)
        review.query_one("#review-agent-output-chat")


@pytest.mark.asyncio
async def test_enter_pair_review_shows_attach_button(tmp_path: Path) -> None:
    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        await _focus_task(pilot, task_ids.pair_review)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)
        review.query_one("#attach-btn")


@pytest.mark.asyncio
async def test_enter_pair_review_auto_starts_ai_review_when_enabled(tmp_path: Path) -> None:
    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        await _focus_task(pilot, task_ids.pair_review)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)
        # Auto-start happens during async hydration; CI runners can lag behind initial mount.
        timeout = 5.0
        elapsed = 0.0
        while elapsed < timeout:
            await pilot.pause()
            if not review.query_one("#ai-review-chat").has_class("hidden"):
                break
            await asyncio.sleep(0.1)
            elapsed += 0.1
        assert not review.query_one("#ai-review-chat").has_class("hidden")


@pytest.mark.asyncio
async def test_enter_done_opens_read_only_review_modal(tmp_path: Path) -> None:
    app, task_ids = await _seed_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        await _focus_task(pilot, task_ids.auto_done)
        await pilot.press("enter")
        review = await wait_for_screen(pilot, ReviewModal, timeout=10.0)
        assert review.query_one(".button-row").has_class("hidden")
