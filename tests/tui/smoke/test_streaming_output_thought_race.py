"""Regression tests for StreamingOutput post_thought / post_response race safety.

Covers the NoneType crash when ``_agent_thought`` is nullified by a concurrent
``post_response`` during the ``await self.mount(...)`` yield point inside
``post_thought``.

Traceback path:
  review.py:on_agent_message -> AgentStreamRouter.dispatch
  -> StreamingOutput.post_thought -> _agent_thought.append_content
  AttributeError: 'NoneType' object has no attribute 'append_content'
"""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult

from kagan.tui.ui.widgets.streaming_output import StreamingOutput


class StreamingOutputApp(App[None]):
    """Minimal app to host a StreamingOutput widget for testing."""

    def compose(self) -> ComposeResult:
        yield StreamingOutput(id="output")


@pytest.mark.asyncio
async def test_post_thought_returns_valid_widget() -> None:
    """post_thought creates and returns a usable StreamingMarkdown widget."""
    app = StreamingOutputApp()
    async with app.run_test(size=(80, 24)):
        output = app.query_one(StreamingOutput)
        widget = await output.post_thought("thinking fragment")
        assert widget is not None
        assert widget.content == "thinking fragment"


@pytest.mark.asyncio
async def test_post_thought_appends_to_existing_thought() -> None:
    """Repeated post_thought calls append to the same thought widget."""
    app = StreamingOutputApp()
    async with app.run_test(size=(80, 24)):
        output = app.query_one(StreamingOutput)
        first = await output.post_thought("part 1")
        second = await output.post_thought(" part 2")
        assert first is second
        assert second.content == "part 1 part 2"


@pytest.mark.asyncio
async def test_post_thought_survives_interleaved_post_response() -> None:
    """post_thought does not crash when post_response nullifies _agent_thought.

    This is the core regression scenario: a response message arrives between
    a thought's mount and its first append_content call.
    """
    app = StreamingOutputApp()
    async with app.run_test(size=(80, 24)):
        output = app.query_one(StreamingOutput)

        # Establish a thought widget.
        thought_widget = await output.post_thought("initial thought")
        assert thought_widget.content == "initial thought"

        # post_response nullifies _agent_thought internally.
        await output.post_response("response text")
        assert output._agent_thought is None

        # Next post_thought must succeed even though _agent_thought is None.
        new_thought = await output.post_thought("new thought after response")
        assert new_thought is not None
        assert new_thought.content == "new thought after response"
        assert new_thought is not thought_widget


@pytest.mark.asyncio
async def test_post_response_survives_interleaved_post_tool_call() -> None:
    """post_response does not crash when post_tool_call nullifies _agent_response.

    Same defensive-reference pattern applied to _agent_response.
    """

    app = StreamingOutputApp()
    async with app.run_test(size=(80, 24)):
        output = app.query_one(StreamingOutput)

        # Establish a response widget.
        response_widget = await output.post_response("initial response")
        assert response_widget.content == "initial response"

        # post_tool_call nullifies _agent_response internally.
        await output.post_tool_call("tc-1", "Read file")
        assert output._agent_response is None

        # Next post_response must succeed even though _agent_response is None.
        new_response = await output.post_response("response after tool call")
        assert new_response is not None
        assert new_response.content == "response after tool call"
        assert new_response is not response_widget


@pytest.mark.asyncio
async def test_rapid_thought_messages_no_crash() -> None:
    """Rapid successive Thinking messages do not cause NoneType errors."""
    app = StreamingOutputApp()
    async with app.run_test(size=(80, 24)):
        output = app.query_one(StreamingOutput)
        for i in range(20):
            widget = await output.post_thought(f"thought-{i} ")
            assert widget is not None


@pytest.mark.asyncio
async def test_rapid_interleaved_thought_response_no_crash() -> None:
    """Rapid alternation between thought and response messages is safe."""
    app = StreamingOutputApp()
    async with app.run_test(size=(80, 24)):
        output = app.query_one(StreamingOutput)
        for i in range(10):
            thought = await output.post_thought(f"thought-{i}")
            assert thought is not None
            response = await output.post_response(f"response-{i}")
            assert response is not None


@pytest.mark.asyncio
async def test_post_thought_after_clear() -> None:
    """post_thought works correctly after output is cleared."""
    app = StreamingOutputApp()
    async with app.run_test(size=(80, 24)):
        output = app.query_one(StreamingOutput)
        await output.post_thought("before clear")
        await output.clear()
        assert output._agent_thought is None

        widget = await output.post_thought("after clear")
        assert widget is not None
        assert widget.content == "after clear"


@pytest.mark.asyncio
async def test_post_thought_after_reset_turn() -> None:
    """post_thought works correctly after reset_turn nullifies _agent_thought."""
    app = StreamingOutputApp()
    async with app.run_test(size=(80, 24)):
        output = app.query_one(StreamingOutput)
        await output.post_thought("before reset")
        output.reset_turn()
        assert output._agent_thought is None

        widget = await output.post_thought("after reset")
        assert widget is not None
        assert widget.content == "after reset"
