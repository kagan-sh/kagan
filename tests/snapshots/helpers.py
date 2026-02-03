"""Helper functions for snapshot testing.

Provides utilities for:
- Keyboard sequence DSL parsing and execution
- Text typing simulation
- Screen waiting and assertion
"""

from __future__ import annotations

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from textual.pilot import Pilot
    from textual.screen import Screen


async def press_sequence(pilot: Pilot, sequence: str) -> None:
    """Parse and execute a keyboard sequence DSL.

    DSL format:
    - `key` = single press (e.g., "enter", "tab", "down")
    - `key(n)` = press n times (e.g., "down(3)" presses down 3 times)
    - `pause` = await pilot.pause()
    - Multiple commands separated by spaces

    Examples:
        "down(3) enter right(2)"
            -> down, down, down, enter, right, right

        "tab(2) pause enter"
            -> tab, tab, pause, enter

        "escape ctrl+c"
            -> escape, ctrl+c

    Args:
        pilot: Textual Pilot instance
        sequence: Space-separated sequence of key commands
    """
    tokens = sequence.split()
    pattern = re.compile(r"^(\w+(?:\+\w+)?)\((\d+)\)$")

    for token in tokens:
        token = token.strip()
        if not token:
            continue

        if token == "pause":
            await pilot.pause()
            continue

        # Check for repeat pattern: key(n)
        match = pattern.match(token)
        if match:
            key = match.group(1)
            count = int(match.group(2))
            for _ in range(count):
                await pilot.press(key)
        else:
            # Single key press
            await pilot.press(token)


async def type_text(pilot: Pilot, text: str, *, delay: float = 0.0) -> None:
    """Type text character by character.

    This simulates typing into a focused input field.

    Args:
        pilot: Textual Pilot instance
        text: Text to type
        delay: Optional delay between characters (for visual testing)
    """
    for char in text:
        await pilot.press(char)
        if delay > 0:
            import asyncio

            await asyncio.sleep(delay)


async def wait_for_screen(
    pilot: Pilot,
    screen_type: type[Screen],
    timeout: float = 5.0,
    check_interval: float = 0.1,
) -> Screen:
    """Wait for a specific screen type to be active.

    Args:
        pilot: Textual Pilot instance
        screen_type: Expected Screen class type
        timeout: Maximum time to wait in seconds
        check_interval: Time between checks in seconds

    Returns:
        The active screen instance

    Raises:
        TimeoutError: If screen doesn't appear within timeout
        AssertionError: If wrong screen type is active after timeout
    """
    import asyncio

    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        current_screen = pilot.app.screen
        if isinstance(current_screen, screen_type):
            return current_screen
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    # Final check with assertion
    current_screen = pilot.app.screen
    assert isinstance(current_screen, screen_type), (
        f"Expected screen {screen_type.__name__}, got {type(current_screen).__name__}"
    )
    return current_screen


async def wait_for_widget(
    pilot: Pilot,
    selector: str,
    timeout: float = 5.0,
    check_interval: float = 0.1,
) -> None:
    """Wait for a widget matching the selector to exist on the current screen.

    Note: This queries from pilot.app.screen, not pilot.app, because
    Textual's app.query_one() doesn't find widgets on screens in some contexts.

    Args:
        pilot: Textual Pilot instance
        selector: CSS selector for the widget
        timeout: Maximum time to wait in seconds
        check_interval: Time between checks in seconds

    Raises:
        TimeoutError: If widget doesn't appear within timeout
    """
    import asyncio

    from textual.css.query import NoMatches

    # Initial pauses to let pending mount operations complete
    await pilot.pause()
    await pilot.pause()

    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        try:
            # Query from the current screen, not app
            # app.query_one() doesn't find screen widgets in some test contexts
            pilot.app.screen.query_one(selector)
            return
        except NoMatches:
            await asyncio.sleep(check_interval)
            elapsed += check_interval
            # Additional pause after sleep to process any pending updates
            await pilot.pause()

    raise TimeoutError(f"Widget '{selector}' not found within {timeout}s")


async def wait_for_planner_ready(
    pilot: Pilot,
    timeout: float = 10.0,
    check_interval: float = 0.1,
) -> None:
    """Wait for the planner agent to be ready.

    The planner screen needs to have agent_ready=True before
    it will accept prompt submissions.

    Args:
        pilot: Textual Pilot instance
        timeout: Maximum time to wait in seconds
        check_interval: Time between checks in seconds

    Raises:
        TimeoutError: If agent doesn't become ready within timeout
    """
    import asyncio

    from kagan.ui.screens.planner import PlannerScreen

    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        screen = pilot.app.screen
        if isinstance(screen, PlannerScreen) and screen._state.agent_ready:
            return
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    raise TimeoutError(f"Planner agent not ready within {timeout}s")


async def wait_for_workers(
    pilot: Pilot,
    timeout: float = 10.0,
    check_interval: float = 0.1,
) -> None:
    """Wait for all background workers to complete.

    This is critical for snapshot tests where actions like submitting
    a prompt spawn background workers that modify the UI.

    Args:
        pilot: Textual Pilot instance
        timeout: Maximum time to wait in seconds
        check_interval: Time between checks in seconds

    Raises:
        TimeoutError: If workers don't complete within timeout
    """
    import asyncio

    from textual.worker import WorkerState

    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        # Check if there are any running workers
        workers = list(pilot.app.workers._workers)
        running = [w for w in workers if w.state in (WorkerState.PENDING, WorkerState.RUNNING)]
        if not running:
            # Workers done - add extra pauses to let mount operations complete
            for _ in range(3):
                await pilot.pause()
            return
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    # One more pause to let any final updates propagate
    await pilot.pause()
    raise TimeoutError(f"Workers did not complete within {timeout}s")


async def wait_for_text(
    pilot: Pilot,
    text: str,
    timeout: float = 5.0,
    check_interval: float = 0.1,
) -> None:
    """Wait for specific text to appear in the app's rendered output.

    Args:
        pilot: Textual Pilot instance
        text: Text to search for
        timeout: Maximum time to wait in seconds
        check_interval: Time between checks in seconds

    Raises:
        TimeoutError: If text doesn't appear within timeout
    """
    import asyncio

    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        # Get rendered text from the app
        rendered = str(pilot.app.screen)
        if text in rendered:
            return
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    raise TimeoutError(f"Text '{text}' not found within {timeout}s")


async def wait_for_modal(
    pilot: Pilot,
    modal_type: type[Screen],
    timeout: float = 5.0,
    check_interval: float = 0.1,
) -> Screen:
    """Wait for a modal screen to be pushed onto the screen stack.

    This is useful for testing modal dialogs that are pushed via push_screen().
    The function checks the entire screen stack, not just the current screen.

    Args:
        pilot: Textual Pilot instance
        modal_type: Expected modal Screen class type
        timeout: Maximum time to wait in seconds
        check_interval: Time between checks in seconds

    Returns:
        The modal screen instance

    Raises:
        TimeoutError: If modal doesn't appear within timeout
    """
    import asyncio

    # Initial pauses to let push_screen complete
    await pilot.pause()
    await pilot.pause()

    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        # Check entire screen stack for the modal
        for screen in pilot.app.screen_stack:
            if isinstance(screen, modal_type):
                # Extra pause to ensure modal is fully rendered
                await pilot.pause()
                return screen
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    raise TimeoutError(f"Modal {modal_type.__name__} not found within {timeout}s")


def assert_snapshot_match(
    snapshot: str,
    expected_elements: list[str],
    *,
    excluded_elements: list[str] | None = None,
) -> None:
    """Assert that snapshot contains expected elements and excludes others.

    Args:
        snapshot: The snapshot text to check
        expected_elements: List of strings that must be present
        excluded_elements: List of strings that must NOT be present

    Raises:
        AssertionError: If any expectation fails
    """
    for element in expected_elements:
        assert element in snapshot, f"Expected '{element}' not found in snapshot"

    if excluded_elements:
        for element in excluded_elements:
            assert element not in snapshot, f"Unexpected '{element}' found in snapshot"
