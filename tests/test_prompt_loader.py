"""Tests for hardcoded prompts module."""

from __future__ import annotations

from kagan.agents.prompt_loader import (
    ITERATION_PROMPT,
    REVIEW_PROMPT,
    get_review_prompt,
)


class TestIterationPrompt:
    """Test iteration prompt for AUTO mode workers."""

    def test_iteration_prompt_has_required_placeholders(self) -> None:
        """Iteration prompt must have all required format placeholders."""
        assert "{iteration}" in ITERATION_PROMPT
        assert "{max_iterations}" in ITERATION_PROMPT
        assert "{title}" in ITERATION_PROMPT
        assert "{description}" in ITERATION_PROMPT
        assert "{scratchpad}" in ITERATION_PROMPT
        assert "{hat_instructions}" in ITERATION_PROMPT

    def test_iteration_prompt_has_signals(self) -> None:
        """Iteration prompt must include required signals."""
        assert "<complete/>" in ITERATION_PROMPT
        assert "<continue/>" in ITERATION_PROMPT
        assert "<blocked" in ITERATION_PROMPT


class TestReviewPrompt:
    """Test review prompt for code review after AUTO completion."""

    def test_review_prompt_has_required_placeholders(self) -> None:
        """Review prompt must have all required format placeholders."""
        assert "{title}" in REVIEW_PROMPT
        assert "{ticket_id}" in REVIEW_PROMPT
        assert "{description}" in REVIEW_PROMPT
        assert "{commits}" in REVIEW_PROMPT
        assert "{diff_summary}" in REVIEW_PROMPT

    def test_review_prompt_has_signals(self) -> None:
        """Review prompt must include required signals."""
        assert "<approve" in REVIEW_PROMPT
        assert "<reject" in REVIEW_PROMPT

    def test_get_review_prompt_formats_correctly(self) -> None:
        """get_review_prompt formats the template with provided values."""
        prompt = get_review_prompt(
            title="Test Ticket",
            ticket_id="abc123",
            description="Test description",
            commits="- feat: added feature",
            diff_summary="+10 -5",
        )
        assert "Test Ticket" in prompt
        assert "abc123" in prompt
        assert "Test description" in prompt
        assert "- feat: added feature" in prompt
        assert "+10 -5" in prompt
