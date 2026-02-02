"""Extended database tests - edge cases not covered by property tests.

See test_database_hypothesis.py for comprehensive property-based tests covering:
- Scratchpad roundtrip, overwrite, truncation, deletion
- New field persistence (acceptance_criteria, checks_passed, session_active, review_summary)
- mark_session_active and set_review_summary operations
- Ticket type persistence and updates
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.integration


class TestScratchpads:
    """Tests for scratchpad edge cases."""

    async def test_get_scratchpad_empty(self, state_manager):
        """Returns empty string for nonexistent scratchpad."""
        result = await state_manager.get_scratchpad("nonexistent")
        assert result == ""
