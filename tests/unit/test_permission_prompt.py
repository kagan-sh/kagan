"""Tests for PermissionPrompt widget."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import MagicMock

import pytest
from textual.app import App, ComposeResult
from textual.containers import Vertical
from textual.widgets import Button, Static

from kagan.ui.widgets.permission_prompt import PermissionPrompt

if TYPE_CHECKING:
    from kagan.acp.messages import Answer

pytestmark = pytest.mark.unit


# =============================================================================
# Test App for PermissionPrompt
# =============================================================================


class PermissionPromptTestApp(App):
    """Test app for PermissionPrompt widget."""

    def __init__(self, prompt: PermissionPrompt):
        super().__init__()
        self._prompt = prompt

    def compose(self) -> ComposeResult:
        yield self._prompt


class PermissionPromptContainerApp(App):
    """Test app with container for mounting PermissionPrompt later."""

    def compose(self) -> ComposeResult:
        yield Vertical(id="container")


# =============================================================================
# Fixtures
# =============================================================================


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


# =============================================================================
# TestPermissionPromptActions - Original Tests
# =============================================================================


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


# =============================================================================
# TestPermissionPromptBindings - Original Tests
# =============================================================================


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


# =============================================================================
# TestPermissionPromptFallback - Original Tests
# =============================================================================


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


# =============================================================================
# TestPermissionPromptCompose - NEW Tests for UI Composition
# =============================================================================


class TestPermissionPromptCompose:
    """Test PermissionPrompt UI composition."""

    async def test_compose_yields_expected_widgets(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that compose() creates header, tool label, buttons, timer."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=10.0,  # Short timeout for testing
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            # Verify header exists
            header = app.query_one(".permission-header", Static)
            assert "Permission Required" in str(header.render())

            # Verify tool label exists
            tool_label = app.query_one(".permission-tool", Static)
            assert "Run Terminal Command" in str(tool_label.render())

            # Verify buttons container exists
            buttons_container = app.query_one(".permission-buttons")
            assert buttons_container is not None

            # Verify timer exists
            timer = app.query_one("#perm-timer", Static)
            assert timer is not None

    async def test_buttons_have_correct_labels(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Verify button labels match expected text."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=10.0,
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            # Verify button IDs and labels
            # Note: Textual interprets [x] as style markup, so we check for text content
            allow_once_btn = app.query_one("#btn-allow-once", Button)
            assert "Allow once" in str(allow_once_btn.label)

            allow_always_btn = app.query_one("#btn-allow-always", Button)
            assert "Allow always" in str(allow_always_btn.label)

            deny_btn = app.query_one("#btn-deny", Button)
            assert "Deny" in str(deny_btn.label)

    async def test_compose_with_unknown_tool_title(
        self,
        sample_options: list[Any],
        result_future: asyncio.Future[Answer],
    ):
        """Test compose with missing tool title uses fallback."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", {"id": "tool-no-title"}),
            result_future=result_future,
            timeout=10.0,
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            tool_label = app.query_one(".permission-tool", Static)
            assert "Unknown Tool" in str(tool_label.render())


# =============================================================================
# TestPermissionPromptTimer - NEW Tests for Countdown Timer
# =============================================================================


class TestPermissionPromptTimer:
    """Test countdown timer functionality."""

    async def test_timer_starts_on_mount(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that timer task is created on mount."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=10.0,
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            # Verify timer task was created
            assert prompt._timer_task is not None
            assert not prompt._timer_task.done()

    async def test_timer_countdown_updates_display(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test remaining_seconds decrements and updates display."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=5.0,  # Short timeout
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            initial_seconds = prompt.remaining_seconds
            assert initial_seconds == 5

            # Wait for a countdown tick
            await asyncio.sleep(1.2)
            await pilot.pause()

            # Verify countdown decreased
            assert prompt.remaining_seconds < initial_seconds

    async def test_timer_expiry_triggers_reject(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that timer reaching 0 auto-rejects."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=1.0,  # Very short timeout
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            # Wait for timeout to expire
            await asyncio.sleep(1.5)
            await pilot.pause()

            # Future should be resolved with reject
            assert result_future.done()
            result = result_future.result()
            assert result.id == "opt-reject"

    async def test_format_timer_formats_correctly(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test _format_timer produces correct string format."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=125.0,  # 2 minutes 5 seconds
        )

        # Test formatting without mounting (just call the method)
        prompt.remaining_seconds = 125
        formatted = prompt._format_timer()
        assert formatted == "Waiting... (2:05)"

        prompt.remaining_seconds = 65
        formatted = prompt._format_timer()
        assert formatted == "Waiting... (1:05)"

        prompt.remaining_seconds = 5
        formatted = prompt._format_timer()
        assert formatted == "Waiting... (0:05)"

        prompt.remaining_seconds = 0
        formatted = prompt._format_timer()
        assert formatted == "Waiting... (0:00)"

    async def test_timer_task_cancelled_on_unmount(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that timer task is cancelled when widget is unmounted."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )
        app = PermissionPromptContainerApp()

        async with app.run_test() as pilot:
            container = app.query_one("#container")
            await container.mount(prompt)
            await pilot.pause()

            timer_task = prompt._timer_task
            assert timer_task is not None
            assert not timer_task.done()

            # Remove the widget
            await prompt.remove()
            await pilot.pause()

            # Timer task should be cancelled
            assert timer_task.cancelled() or timer_task.done()

    async def test_watch_remaining_seconds_updates_timer_label(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that watch_remaining_seconds updates the timer label."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            # Manually change remaining_seconds to trigger watcher
            prompt.remaining_seconds = 30
            await pilot.pause()

            timer = app.query_one("#perm-timer", Static)
            assert "0:30" in str(timer.render())

    async def test_watch_remaining_seconds_handles_missing_widget(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that watch_remaining_seconds handles missing timer widget gracefully."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )

        # Call watch_remaining_seconds without mounting (timer widget doesn't exist)
        # Should not raise an exception
        prompt.watch_remaining_seconds()


# =============================================================================
# TestPermissionPromptButtonHandlers - NEW Tests for Button Click Handlers
# =============================================================================


class TestPermissionPromptButtonHandlers:
    """Test button click handlers."""

    async def test_button_allow_once_click(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test clicking Allow once button resolves correctly."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            # Create mock button pressed event
            btn = app.query_one("#btn-allow-once", Button)
            event = MagicMock()
            event.button = btn

            prompt.on_button_pressed(event)

            assert event.stop.called
            assert result_future.done()
            result = result_future.result()
            assert result.id == "opt-allow-once"

    async def test_button_allow_always_click(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test clicking Allow always button resolves correctly."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            btn = app.query_one("#btn-allow-always", Button)
            event = MagicMock()
            event.button = btn

            prompt.on_button_pressed(event)

            assert event.stop.called
            assert result_future.done()
            result = result_future.result()
            assert result.id == "opt-allow-always"

    async def test_button_deny_click(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test clicking Deny button resolves correctly."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            btn = app.query_one("#btn-deny", Button)
            event = MagicMock()
            event.button = btn

            prompt.on_button_pressed(event)

            assert event.stop.called
            assert result_future.done()
            result = result_future.result()
            assert result.id == "opt-reject"

    async def test_button_unknown_id_does_nothing(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that clicking unknown button ID does nothing."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            # Create a mock button with unknown ID
            mock_btn = MagicMock()
            mock_btn.id = "btn-unknown"
            event = MagicMock()
            event.button = mock_btn

            prompt.on_button_pressed(event)

            # Event should still be stopped
            assert event.stop.called
            # But future should NOT be resolved
            assert not result_future.done()


# =============================================================================
# TestPermissionPromptMountUnmount - NEW Tests for Mount/Unmount Lifecycle
# =============================================================================


class TestPermissionPromptMountUnmount:
    """Test mount and unmount lifecycle."""

    async def test_on_mount_sets_focus(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that on_mount focuses the widget."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )
        app = PermissionPromptTestApp(prompt)

        async with app.run_test() as pilot:
            await pilot.pause()

            # The widget should be focusable and timer should be set
            assert prompt.remaining_seconds == 60

    async def test_on_unmount_rejects_if_not_resolved(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that unmounting without resolution triggers reject."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )
        app = PermissionPromptContainerApp()

        async with app.run_test() as pilot:
            container = app.query_one("#container")
            await container.mount(prompt)
            await pilot.pause()

            assert not result_future.done()

            # Unmount without resolving
            await prompt.remove()
            await pilot.pause()

            # Future should be rejected
            assert result_future.done()
            result = result_future.result()
            assert result.id == "opt-reject"

    async def test_on_unmount_does_not_reject_if_already_resolved(
        self,
        sample_options: list[Any],
        sample_tool_call: dict[str, str],
        result_future: asyncio.Future[Answer],
    ):
        """Test that unmounting after resolution doesn't change result."""
        prompt = PermissionPrompt(
            options=cast("Any", sample_options),
            tool_call=cast("Any", sample_tool_call),
            result_future=result_future,
            timeout=60.0,
        )
        app = PermissionPromptContainerApp()

        async with app.run_test() as pilot:
            container = app.query_one("#container")
            await container.mount(prompt)
            await pilot.pause()

            # Resolve with allow_once
            prompt.action_allow_once()
            assert result_future.done()
            original_result = result_future.result()
            assert original_result.id == "opt-allow-once"

            # Now unmount
            await prompt.remove()
            await pilot.pause()

            # Result should still be allow_once (not changed to reject)
            assert result_future.result().id == "opt-allow-once"
