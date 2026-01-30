"""Tests for streaming conversation widgets."""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult
from textual.containers import Vertical

from kagan.ui.widgets import AgentResponse, AgentThought, StreamingOutput, ToolCall, UserInput

pytestmark = pytest.mark.e2e


class WidgetTestApp(App):
    def compose(self) -> ComposeResult:
        yield Vertical(id="container")


class TestUserInput:
    async def test_displays_prompt_and_content(self):
        app = WidgetTestApp()
        async with app.run_test() as pilot:
            await app.query_one("#container").mount(UserInput("Hello"))
            await pilot.pause()
            w = app.query_one(UserInput)
            assert w.query_one(".user-input-prompt") and w.query_one(".user-input-content")


class TestAgentResponse:
    async def test_append_fragment(self):
        app = WidgetTestApp()
        async with app.run_test() as pilot:
            r = AgentResponse("Init")
            await app.query_one("#container").mount(r)
            await r.append_fragment(" more")
            await pilot.pause()
            assert r.is_attached


class TestAgentThought:
    async def test_append_fragment(self):
        app = WidgetTestApp()
        async with app.run_test() as pilot:
            t = AgentThought("Think")
            await app.query_one("#container").mount(t)
            await t.append_fragment(" more")
            await pilot.pause()
            assert t.is_attached


class TestToolCall:
    async def test_status_update_and_expand(self):
        app = WidgetTestApp()
        async with app.run_test() as pilot:
            data = {
                "id": "t1",
                "title": "T",
                "status": "pending",
                "content": [{"type": "content", "content": {"type": "text", "text": "X"}}],
            }
            tool = ToolCall(data)
            await app.query_one("#container").mount(tool)
            await pilot.pause()
            assert tool.tool_call["status"] == "pending"
            tool.update_status("completed")
            assert tool.tool_call["status"] == "completed"
            assert not tool.expanded
            tool.expanded = True
            assert tool.expanded


class TestStreamingOutput:
    async def test_post_user_input(self):
        app = WidgetTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await output.post_user_input("Hi")
            await pilot.pause()
            assert output.query_one(UserInput)

    async def test_post_response_resets_thought(self):
        app = WidgetTestApp()
        async with app.run_test():
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await output.post_thought("...")
            assert output._agent_thought is not None
            await output.post_response("resp")
            assert output._agent_thought is None

    async def test_clear_removes_children(self):
        app = WidgetTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await output.post_user_input("x")
            await output.post_response("y")
            await output.clear()
            await pilot.pause()
            assert len(output.children) == 0
