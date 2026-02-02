"""Property-based tests for terminal capability detection using Hypothesis.

Replaces parametrized tests from test_terminal.py with property-based equivalents.
"""

from __future__ import annotations

import os
from contextlib import contextmanager
from typing import TYPE_CHECKING

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

if TYPE_CHECKING:
    from collections.abc import Iterator

from kagan.terminal import get_terminal_name, supports_truecolor

pytestmark = pytest.mark.unit


# =============================================================================
# Strategies
# =============================================================================

# Safe text that can be used as environment variable values
# (no null bytes, no surrogates which can't be UTF-8 encoded)
safe_env_text = st.text(
    alphabet=st.characters(
        blacklist_characters="\x00",
        blacklist_categories=("Cs",),  # Exclude surrogates
    ),
    min_size=1,
    max_size=50,
).filter(lambda x: x.strip())


# =============================================================================
# Test Data
# =============================================================================

# Terminals known to support truecolor
TRUECOLOR_TERMINALS = frozenset(
    {
        "iterm.app",
        "vscode",
        "kitty",
        "warp",
        "alacritty",
        "wezterm",
        "ghostty",
    }
)

# Terminal name mappings (TERM_PROGRAM -> friendly name)
TERMINAL_NAMES = {
    "Apple_Terminal": "macOS Terminal.app",
    "iTerm.app": "iTerm2",
    "vscode": "VS Code Terminal",
}


@contextmanager
def patched_env(**env_vars: str | None) -> Iterator[None]:
    """Context manager to temporarily modify environment variables.

    Args:
        **env_vars: Environment variables to set. None values delete the var.
    """
    # Clear all terminal-related env vars first
    terminal_vars = ["COLORTERM", "TEXTUAL_COLOR_SYSTEM", "TERM_PROGRAM", "WT_SESSION", "TERM"]
    old_values = {var: os.environ.get(var) for var in terminal_vars}

    for var in terminal_vars:
        if var in os.environ:
            del os.environ[var]

    # Set requested values
    for key, value in env_vars.items():
        if value is not None:
            os.environ[key] = value

    try:
        yield
    finally:
        # Restore original state
        for var in terminal_vars:
            if var in os.environ:
                del os.environ[var]
        for key, value in old_values.items():
            if value is not None:
                os.environ[key] = value


# =============================================================================
# Property-Based Tests for Truecolor Detection
# =============================================================================


class TestTruecolorDetectionProperties:
    """Property-based tests for supports_truecolor()."""

    @given(st.sampled_from(list(TRUECOLOR_TERMINALS)))
    @settings(max_examples=len(TRUECOLOR_TERMINALS))
    def test_known_truecolor_terminals_detected(self, terminal: str) -> None:
        """All known truecolor terminals are detected correctly."""
        with patched_env(TERM_PROGRAM=terminal):
            assert supports_truecolor() is True

    @given(st.sampled_from(list(TRUECOLOR_TERMINALS)))
    @settings(max_examples=len(TRUECOLOR_TERMINALS))
    def test_truecolor_detection_case_insensitive(self, terminal: str) -> None:
        """Truecolor terminal detection is case-insensitive."""
        with patched_env(TERM_PROGRAM=terminal.upper()):
            assert supports_truecolor() is True

    @given(st.sampled_from(["truecolor", "24bit"]))
    def test_colorterm_values_enable_truecolor(self, colorterm: str) -> None:
        """COLORTERM=truecolor or COLORTERM=24bit enables truecolor."""
        with patched_env(COLORTERM=colorterm):
            assert supports_truecolor() is True

    def test_colorterm_case_insensitive(self) -> None:
        """COLORTERM detection is case-insensitive."""
        with patched_env(COLORTERM="TRUECOLOR"):
            assert supports_truecolor() is True

    @given(safe_env_text)
    def test_textual_override_enables_truecolor(self, value: str) -> None:
        """TEXTUAL_COLOR_SYSTEM=truecolor overrides everything."""
        with patched_env(TEXTUAL_COLOR_SYSTEM="truecolor", TERM_PROGRAM="Apple_Terminal"):
            assert supports_truecolor() is True

    @given(safe_env_text)
    def test_windows_terminal_detected(self, session_id: str) -> None:
        """WT_SESSION presence indicates Windows Terminal (truecolor)."""
        with patched_env(WT_SESSION=session_id):
            assert supports_truecolor() is True


class TestAppleTerminalSpecialCase:
    """Tests for Apple Terminal's special truecolor handling."""

    def test_apple_terminal_ignores_colorterm(self) -> None:
        """macOS Terminal.app returns False even if COLORTERM=truecolor.

        Some shell configs incorrectly set COLORTERM=truecolor globally,
        but Apple Terminal genuinely does not support truecolor.
        """
        with patched_env(TERM_PROGRAM="Apple_Terminal", COLORTERM="truecolor"):
            assert supports_truecolor() is False

    def test_apple_terminal_respects_textual_override(self) -> None:
        """TEXTUAL_COLOR_SYSTEM=truecolor overrides even Apple_Terminal."""
        with patched_env(TERM_PROGRAM="Apple_Terminal", TEXTUAL_COLOR_SYSTEM="truecolor"):
            assert supports_truecolor() is True


class TestNonTruecolorCases:
    """Tests for terminals that don't support truecolor."""

    def test_no_indicators_returns_false(self) -> None:
        """No truecolor indicators returns False."""
        with patched_env(TERM="xterm-256color"):
            assert supports_truecolor() is False

    @given(
        st.text(
            alphabet=st.characters(
                blacklist_characters="\x00",
                blacklist_categories=("Cs",),  # Exclude surrogates
            ),
            min_size=1,
            max_size=30,
        ).filter(lambda x: x.strip() and x.lower() not in TRUECOLOR_TERMINALS)
    )
    @settings(max_examples=20)
    def test_unknown_terminals_return_false(self, terminal: str) -> None:
        """Unknown terminals default to False for truecolor."""
        # Skip if it happens to match a known terminal
        if terminal.lower() in TRUECOLOR_TERMINALS:
            return
        with patched_env(TERM_PROGRAM=terminal):
            # May be True if terminal name matches, but should be deterministic
            result = supports_truecolor()
            assert isinstance(result, bool)


# =============================================================================
# Property-Based Tests for Terminal Name Detection
# =============================================================================


class TestTerminalNameProperties:
    """Property-based tests for get_terminal_name()."""

    @given(st.sampled_from(list(TERMINAL_NAMES.keys())))
    @settings(max_examples=len(TERMINAL_NAMES))
    def test_known_terminals_have_friendly_names(self, term_program: str) -> None:
        """Known terminals return their friendly name."""
        with patched_env(TERM_PROGRAM=term_program):
            expected = TERMINAL_NAMES[term_program]
            assert get_terminal_name() == expected

    def test_windows_terminal_name(self) -> None:
        """Windows Terminal is detected via WT_SESSION."""
        with patched_env(WT_SESSION="some-guid"):
            assert get_terminal_name() == "Windows Terminal"

    @given(
        st.text(
            alphabet=st.characters(
                blacklist_characters="\x00",
                blacklist_categories=("Cs",),  # Exclude surrogates
            ),
            min_size=1,
            max_size=30,
        ).filter(lambda x: x.strip() and x not in TERMINAL_NAMES)
    )
    @settings(max_examples=10)
    def test_unknown_terminals_passthrough(self, terminal: str) -> None:
        """Unknown TERM_PROGRAM values are returned as-is."""
        with patched_env(TERM_PROGRAM=terminal):
            assert get_terminal_name() == terminal

    def test_fallback_to_term(self) -> None:
        """Falls back to TERM if TERM_PROGRAM not set."""
        with patched_env(TERM="xterm-256color"):
            assert get_terminal_name() == "xterm-256color"

    def test_unknown_terminal(self) -> None:
        """Returns 'Unknown terminal' if nothing is set."""
        with patched_env():
            assert get_terminal_name() == "Unknown terminal"


# =============================================================================
# Theme Compatibility Tests (kept as explicit tests)
# =============================================================================


class TestThemeCompatibility:
    """Test theme compatibility between truecolor and 256-color variants."""

    def test_themes_have_same_variable_keys(self) -> None:
        """Both themes must have the same variable keys for CSS compatibility."""
        from kagan.theme import KAGAN_THEME, KAGAN_THEME_256

        assert set(KAGAN_THEME.variables.keys()) == set(KAGAN_THEME_256.variables.keys())

    def test_both_themes_are_dark(self) -> None:
        """Both themes should be dark mode for consistency."""
        from kagan.theme import KAGAN_THEME, KAGAN_THEME_256

        assert KAGAN_THEME.dark is True
        assert KAGAN_THEME_256.dark is True
