"""Inline permission request widget with countdown timer."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from textual.containers import Horizontal, VerticalGroup
from textual.reactive import var
from textual.widgets import Button, Static

from kagan.keybindings import PERMISSION_PROMPT_BINDINGS, to_textual_bindings

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.acp import protocol
    from kagan.acp.messages import Answer


class PermissionPrompt(VerticalGroup):
    """Permission request widget with countdown and keyboard bindings."""

    DEFAULT_CLASSES = "permission-prompt"
    BINDINGS = to_textual_bindings(PERMISSION_PROMPT_BINDINGS)

    remaining_seconds: var[int] = var(300)

    def __init__(
        self,
        options: list[protocol.PermissionOption],
        tool_call: protocol.ToolCall | protocol.ToolCallUpdatePermissionRequest,
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
        self._timer_task: asyncio.Task[None] | None = None

    @property
    def title(self) -> str:
        return self._tool_call.get("title") or "Unknown Tool"

    def compose(self) -> ComposeResult:
        yield Static("⚠️ Permission Required", classes="permission-header")
        yield Static(f"Tool: {self.title}", classes="permission-tool")
        with Horizontal(classes="permission-buttons"):
            yield Button("[y] Allow once", id="btn-allow-once", variant="success")
            yield Button("[a] Allow always", id="btn-allow-always", variant="warning")
            yield Button("[n] Deny", id="btn-deny", variant="error")
        yield Static(self._format_timer(), id="perm-timer", classes="permission-timer")

    def on_mount(self) -> None:
        self.remaining_seconds = self._timeout
        self._timer_task = asyncio.create_task(self._countdown())
        self.focus()

    def on_unmount(self) -> None:
        if self._timer_task and not self._timer_task.done():
            self._timer_task.cancel()
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
        except Exception:
            pass

    def _format_timer(self) -> str:
        mins, secs = divmod(self.remaining_seconds, 60)
        return f"Waiting... ({mins}:{secs:02d})"

    def _find_option_id(self, kind: str) -> str | None:
        for opt in self._options:
            if opt.get("kind") == kind:
                return opt.get("optionId")
        return None

    def _resolve(self, option_id: str) -> None:
        if self._result_future.done():
            return
        from kagan.acp.messages import Answer

        self._result_future.set_result(Answer(id=option_id))
        self.call_later(self.remove)

    def _reject(self) -> None:
        option_id = self._find_option_id("reject_once")
        if option_id:
            self._resolve(option_id)
        elif not self._result_future.done():
            from kagan.acp.messages import Answer

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
