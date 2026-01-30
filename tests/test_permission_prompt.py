"""Tests for PermissionPrompt widget."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any, cast

import pytest

from kagan.ui.widgets.permission_prompt import PermissionPrompt

if TYPE_CHECKING:
    from kagan.acp.messages import Answer

pytestmark = pytest.mark.unit


@pytest.fixture
def sample_options() -> list[Any]:
    return [
        {"kind": "allow_once", "name": "Allow once", "optionId": "opt-allow-once"},
        {"kind": "allow_always", "name": "Allow always", "optionId": "opt-allow-always"},
        {"kind": "reject_once", "name": "Deny", "optionId": "opt-reject"},
    ]


@pytest.fixture
def sample_tool_call() -> dict[str, str]:
    return {"id": "tool-123", "title": "Run Terminal Command", "kind": "terminal"}


@pytest.fixture
async def result_future() -> asyncio.Future[Answer]:
    """Create a future for testing in the current event loop."""
    return asyncio.get_running_loop().create_future()


class TestPermissionPromptActions:
    """Tests for PermissionPrompt action methods."""

    async def test_allow_once_resolves_future(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that action_allow_once resolves future with correct option ID."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
        )

        prompt.action_allow_once()

        assert result_future.done()
        result = result_future.result()
        assert result.id == "opt-allow-once"

    async def test_allow_always_resolves_future(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that action_allow_always resolves future with correct option ID."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
        )

        prompt.action_allow_always()

        assert result_future.done()
        result = result_future.result()
        assert result.id == "opt-allow-always"

    async def test_deny_resolves_future(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that action_deny resolves future with reject option ID."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
        )

        prompt.action_deny()

        assert result_future.done()
        result = result_future.result()
        assert result.id == "opt-reject"


class TestPermissionPromptBindings:
    """Tests for PermissionPrompt keyboard bindings."""

    def test_keyboard_bindings_exist(self):
        """Verify BINDINGS list has y, a, n, escape keys."""
        from textual.binding import Binding

        bindings = PermissionPrompt.BINDINGS
        binding_keys: list[str] = []
        for b in bindings:
            if isinstance(b, Binding):
                binding_keys.append(b.key)
            elif isinstance(b, tuple):
                binding_keys.append(b[0])

        assert "y" in binding_keys
        assert "a" in binding_keys
        assert "n" in binding_keys
        assert "escape" in binding_keys


class TestPermissionPromptFallback:
    """Tests for PermissionPrompt fallback behavior."""

    async def test_fallback_when_option_missing(
        self,
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test fallback when preferred kind not available."""
        limited_options: list[Any] = [
            {"kind": "allow_always", "name": "Allow always", "optionId": "opt-always"},
            {"kind": "reject_once", "name": "Deny", "optionId": "opt-reject"},
        ]
        prompt = PermissionPrompt(
            options=cast("Any", limited_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
        )

        # action_allow_once should not resolve future when kind missing
        prompt.action_allow_once()

        assert not result_future.done()

    async def test_reject_fallback_when_reject_missing(
        self,
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that _reject sets empty ID when reject_once not available."""
        limited_options: list[Any] = [
            {"kind": "allow_once", "name": "Allow once", "optionId": "opt-once"},
        ]
        prompt = PermissionPrompt(
            options=cast("Any", limited_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
        )

        prompt._reject()

        assert result_future.done()
        result = result_future.result()
        assert result.id == ""

    async def test_title_property_returns_tool_title(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test title property returns tool call title."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
        )

        assert prompt.title == "Run Terminal Command"

    async def test_title_property_fallback(
        self,
        sample_options: list[Any],
        result_future: asyncio.Future[Answer],
    ):
        """Test title property returns fallback when title missing."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", {"id": "tool-456"}),
            result_future=result_future,
        )

        assert prompt.title == "Unknown Tool"
