"""Inline permission request widget with countdown timer."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from textual.containers import Horizontal, VerticalGroup
from textual.css.query import NoMatches
from textual.reactive import var
from textual.widgets import Button, Static

from kagan.tui.keybindings import PERMISSION_PROMPT_BINDINGS
from kagan.tui.ui.user_messages import (
    PERMISSION_ACTION_HINT,
    PERMISSION_HEADER,
    permission_timer_line,
    permission_tool_line,
)

if TYPE_CHECKING:
    from acp.schema import PermissionOption, ToolCall, ToolCallUpdate
    from textual.app import ComposeResult
    from textual.worker import Worker

    from kagan.core.acp.messages import Answer


class PermissionPrompt(VerticalGroup):
    """Permission request widget with countdown and keyboard bindings."""

    DEFAULT_CLASSES = "permission-prompt"
    BINDINGS = PERMISSION_PROMPT_BINDINGS

    remaining_seconds: var[int] = var(300)

    def __init__(
        self,
        options: list[PermissionOption],
        tool_call: ToolCall | ToolCallUpdate,
        result_future: asyncio.Future[Answer],
        timeout: float = 300.0,
        *,
        id: str | None = None,
        classes: str | None = None,
    ) -> None:
        super().__init__(id=id, classes=classes)
        self._options = options
        self._tool_call = tool_call
        self._result_future = result_future
        self._timeout = int(timeout)
        self._timer_worker: Worker[None] | None = None

    @property
    def title(self) -> str:
        title = getattr(self._tool_call, "title", None)
        return title or "Unknown Tool"

    def compose(self) -> ComposeResult:
        yield Static(f"⚠ {PERMISSION_HEADER}", classes="permission-header")
        yield Static(permission_tool_line(self.title), classes="permission-tool")
        with Horizontal(classes="permission-buttons"):
            yield Button("[Enter] Allow once", id="btn-allow-once", variant="success")
            yield Button("[A] Allow always", id="btn-allow-always", variant="warning")
            yield Button("[Esc] Deny", id="btn-deny", variant="error")
        yield Static(PERMISSION_ACTION_HINT, classes="permission-tool")
        yield Static(self._format_timer(), id="perm-timer", classes="permission-timer")

    def on_mount(self) -> None:
        self.remaining_seconds = self._timeout
        self._timer_worker = self.run_worker(
            self._countdown(),
            exclusive=True,
            exit_on_error=False,
        )
        self.focus()

    def on_unmount(self) -> None:
        if self._timer_worker is not None and not self._timer_worker.is_finished:
            self._timer_worker.cancel()
        if not self._result_future.done():
            self._reject()

    async def _countdown(self) -> None:
        while self.remaining_seconds > 0:
            await asyncio.sleep(1)
            self.remaining_seconds -= 1
        if not self._result_future.done():
            self._reject()
            self.call_later(self.remove)

    def watch_remaining_seconds(self) -> None:
        try:
            timer = self.query_one("#perm-timer", Static)
            timer.update(self._format_timer())
        except NoMatches:
            return

    def _format_timer(self) -> str:
        return permission_timer_line(self.remaining_seconds)

    def _find_option_id(self, kind: str) -> str | None:
        for opt in self._options:
            if opt.kind == kind:
                return opt.option_id
        return None

    def _resolve(self, option_id: str) -> None:
        if self._result_future.done():
            return
        from kagan.core.acp.messages import Answer

        self._result_future.set_result(Answer(id=option_id))
        self.call_later(self.remove)

    def _reject(self) -> None:
        option_id = self._find_option_id("reject_once")
        if option_id:
            self._resolve(option_id)
        elif not self._result_future.done():
            from kagan.core.acp.messages import Answer

            self._result_future.set_result(Answer(id=""))

    def action_allow_once(self) -> None:
        option_id = self._find_option_id("allow_once")
        if option_id:
            self._resolve(option_id)

    def action_allow_always(self) -> None:
        option_id = self._find_option_id("allow_always")
        if option_id:
            self._resolve(option_id)

    def action_deny(self) -> None:
        self._reject()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        event.stop()
        if event.button.id == "btn-allow-once":
            self.action_allow_once()
        elif event.button.id == "btn-allow-always":
            self.action_allow_always()
        elif event.button.id == "btn-deny":
            self.action_deny()
