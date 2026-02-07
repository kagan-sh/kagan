from __future__ import annotations

import asyncio
from datetime import datetime
from typing import TYPE_CHECKING

import pytest
from tests.helpers.wait import wait_for_screen
from textual.app import App, ComposeResult
from textual.widgets import Button, Input, Select, Static

from kagan.config import KaganConfig
from kagan.core.models.entities import Task
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from kagan.ui.modals.settings import SettingsModal
from kagan.ui.modals.task_details_modal import TaskDetailsModal
from kagan.ui.screens.task_editor import TaskEditorScreen

if TYPE_CHECKING:
    from pathlib import Path


class _ModalHarnessApp(App[None]):
    def __init__(self, config: KaganConfig) -> None:
        super().__init__()
        self.config = config
        self.kagan_app = self
        self._ctx = None

    def compose(self) -> ComposeResult:
        yield Static("host")


@pytest.mark.asyncio
async def test_task_details_modal_persists_pair_terminal_backend() -> None:
    app = _ModalHarnessApp(KaganConfig())

    async with app.run_test(size=(120, 40)) as pilot:
        loop = asyncio.get_running_loop()
        result_future: asyncio.Future[object | None] = loop.create_future()
        pilot.app.push_screen(
            TaskDetailsModal(),
            callback=lambda result: result_future.set_result(result),
        )
        modal = await wait_for_screen(pilot, TaskDetailsModal, timeout=5.0)

        modal.query_one("#title-input", Input).value = "PAIR terminal test"
        terminal_select = modal.query_one("#pair-terminal-backend-select", Select)
        terminal_select.value = "cursor"
        await pilot.pause()
        modal.query_one("#save-btn", Button).press()

        result = await result_future

    assert isinstance(result, dict)
    assert result["task_type"] == TaskType.PAIR
    assert result["terminal_backend"] == "cursor"


@pytest.mark.asyncio
async def test_task_details_modal_clears_terminal_backend_for_auto_task() -> None:
    app = _ModalHarnessApp(KaganConfig())

    async with app.run_test(size=(120, 40)) as pilot:
        loop = asyncio.get_running_loop()
        result_future: asyncio.Future[object | None] = loop.create_future()
        pilot.app.push_screen(
            TaskDetailsModal(),
            callback=lambda result: result_future.set_result(result),
        )
        modal = await wait_for_screen(pilot, TaskDetailsModal, timeout=5.0)

        modal.query_one("#title-input", Input).value = "AUTO terminal test"
        type_select = modal.query_one("#type-select", Select)
        type_select.value = TaskType.AUTO.value
        await pilot.pause()
        modal.query_one("#save-btn", Button).press()

        result = await result_future

    assert isinstance(result, dict)
    assert result["task_type"] == TaskType.AUTO
    assert result["terminal_backend"] is None


@pytest.mark.asyncio
async def test_task_editor_toggles_terminal_backend_with_type_change() -> None:
    now = datetime.now()
    task = Task(
        id="task-1",
        project_id="proj-1",
        title="Editor task",
        description="",
        status=TaskStatus.BACKLOG,
        priority=TaskPriority.MEDIUM,
        task_type=TaskType.PAIR,
        created_at=now,
        updated_at=now,
    )
    app = _ModalHarnessApp(KaganConfig())

    async with app.run_test(size=(120, 40)) as pilot:
        loop = asyncio.get_running_loop()
        result_future: asyncio.Future[list[Task] | None] = loop.create_future()
        pilot.app.push_screen(
            TaskEditorScreen([task]),
            callback=lambda result: result_future.set_result(result),
        )
        editor = await wait_for_screen(pilot, TaskEditorScreen, timeout=5.0)

        type_select = editor.query_one("#type-1", Select)
        terminal_select = editor.query_one("#terminal-backend-1", Select)
        assert terminal_select.disabled is False

        type_select.value = TaskType.AUTO.value
        await pilot.pause()
        assert terminal_select.disabled

        type_select.value = TaskType.PAIR.value
        terminal_select.value = "cursor"
        await pilot.pause()
        editor.query_one("#finish-btn", Button).press()

        edited = await result_future

    assert edited is not None
    if "terminal_backend" in Task.model_fields:
        assert edited[0].terminal_backend == "cursor"


@pytest.mark.asyncio
async def test_task_editor_invalid_pair_backend_falls_back_to_tmux() -> None:
    now = datetime.now()
    task = Task.model_construct(
        id="task-2",
        project_id="proj-1",
        title="Editor invalid backend task",
        description="",
        status=TaskStatus.BACKLOG,
        priority=TaskPriority.MEDIUM,
        task_type=TaskType.PAIR,
        terminal_backend="invalid-launcher",
        assigned_hat=None,
        agent_backend=None,
        parent_id=None,
        acceptance_criteria=[],
        base_branch=None,
        created_at=now,
        updated_at=now,
    )
    app = _ModalHarnessApp(KaganConfig())

    async with app.run_test(size=(120, 40)) as pilot:
        loop = asyncio.get_running_loop()
        result_future: asyncio.Future[list[Task] | None] = loop.create_future()
        pilot.app.push_screen(
            TaskEditorScreen([task]),
            callback=lambda result: result_future.set_result(result),
        )
        editor = await wait_for_screen(pilot, TaskEditorScreen, timeout=5.0)

        editor.query_one("#finish-btn", Button).press()
        edited = await result_future

    assert edited is not None
    if "terminal_backend" in Task.model_fields:
        assert edited[0].terminal_backend == "tmux"


@pytest.mark.asyncio
async def test_settings_modal_updates_default_pair_terminal_backend(tmp_path: Path) -> None:
    config = KaganConfig()
    config_path = tmp_path / "config.toml"
    app = _ModalHarnessApp(config)

    async with app.run_test(size=(120, 40)) as pilot:
        loop = asyncio.get_running_loop()
        result_future: asyncio.Future[object | None] = loop.create_future()
        pilot.app.push_screen(
            SettingsModal(config, config_path),
            callback=lambda result: result_future.set_result(result),
        )
        modal = await wait_for_screen(pilot, SettingsModal, timeout=5.0)

        select = modal.query_one("#default-pair-terminal-select", Select)
        select.value = "tmux"
        await pilot.pause()
        modal.query_one("#save-btn", Button).press()

        result = await result_future

    assert result is True
    assert config.general.default_pair_terminal_backend == "tmux"


@pytest.mark.asyncio
async def test_settings_modal_updates_additional_default_models(tmp_path: Path) -> None:
    config = KaganConfig()
    config_path = tmp_path / "config.toml"
    app = _ModalHarnessApp(config)

    async with app.run_test(size=(120, 40)) as pilot:
        loop = asyncio.get_running_loop()
        result_future: asyncio.Future[object | None] = loop.create_future()
        pilot.app.push_screen(
            SettingsModal(config, config_path),
            callback=lambda result: result_future.set_result(result),
        )
        modal = await wait_for_screen(pilot, SettingsModal, timeout=5.0)

        modal.query_one("#default-model-codex-input", Input).value = "gpt-5.2-codex"
        modal.query_one("#default-model-gemini-input", Input).value = "flash"
        modal.query_one("#default-model-kimi-input", Input).value = "kimi-k2-turbo-preview"
        modal.query_one("#default-model-copilot-input", Input).value = "GPT-5"
        await pilot.pause()
        modal.query_one("#save-btn", Button).press()

        result = await result_future

    assert result is True
    assert config.general.default_model_codex == "gpt-5.2-codex"
    assert config.general.default_model_gemini == "flash"
    assert config.general.default_model_kimi == "kimi-k2-turbo-preview"
    assert config.general.default_model_copilot == "GPT-5"
