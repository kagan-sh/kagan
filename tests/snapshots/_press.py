from __future__ import annotations

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from textual.pilot import Pilot


async def press_sequence(pilot: Pilot, sequence: str) -> None:
    """Parse and execute a keyboard sequence DSL.

    DSL format:
    - ``key`` = single press (e.g. "enter", "tab", "down")
    - ``key(n)`` = press *n* times (e.g. "down(3)" presses down 3 times)
    - ``pause`` = await pilot.pause()
    - Multiple commands separated by spaces
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

        match = pattern.match(token)
        if match:
            key = match.group(1)
            count = int(match.group(2))
            for _ in range(count):
                await pilot.press(key)
        else:
            await pilot.press(token)


def assert_snapshot_match(
    snapshot: str,
    expected_elements: list[str],
    *,
    excluded_elements: list[str] | None = None,
) -> None:
    for element in expected_elements:
        assert element in snapshot, f"Expected '{element}' not found in snapshot"

    if excluded_elements:
        for element in excluded_elements:
            assert element not in snapshot, f"Unexpected '{element}' found in snapshot"
