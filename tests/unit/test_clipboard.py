"""Unit tests for clipboard utilities."""

from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest

from kagan.ui.utils.clipboard import copy_with_notification

pytestmark = pytest.mark.unit


class TestCopyWithNotification:
    """Tests for copy_with_notification function."""

    def test_empty_text_shows_warning(self):
        """Returns False and shows warning for empty text."""
        app = MagicMock()

        result = copy_with_notification(app, "")

        assert result is False
        app.notify.assert_called_once()
        assert "Nothing to copy" in app.notify.call_args[0][0]
        assert app.notify.call_args[1]["severity"] == "warning"

    def test_whitespace_only_shows_warning(self):
        """Returns False and shows warning for whitespace-only text."""
        app = MagicMock()

        result = copy_with_notification(app, "   \n\t  ")

        assert result is False
        app.notify.assert_called_once()
        assert "Nothing to copy" in app.notify.call_args[0][0]

    @pytest.mark.parametrize(
        "text,label",
        [
            ("Hello World", "Content"),
            ("Some code\nwith newlines", "Code"),
            ("x" * 100, "Diff"),
        ],
    )
    def test_successful_copy_notifies_and_returns_true(self, text, label):
        """Successful copy returns True and shows notification."""
        app = MagicMock()

        with patch("kagan.ui.utils.clipboard.pyperclip.copy") as mock_copy:
            result = copy_with_notification(app, text, label=label)

        assert result is True
        mock_copy.assert_called_once_with(text)
        app.notify.assert_called_once()
        assert label in app.notify.call_args[0][0]
        assert "clipboard" in app.notify.call_args[0][0]

    def test_pyperclip_exception_shows_error(self):
        """Pyperclip exception returns False and shows error notification."""
        import pyperclip

        app = MagicMock()

        with patch(
            "kagan.ui.utils.clipboard.pyperclip.copy",
            side_effect=pyperclip.PyperclipException("No clipboard mechanism"),
        ):
            result = copy_with_notification(app, "test text")

        assert result is False
        app.notify.assert_called_once()
        assert "Copy failed" in app.notify.call_args[0][0]
        assert app.notify.call_args[1]["severity"] == "error"

    def test_default_label_is_content(self):
        """Default label is 'Content' when not specified."""
        app = MagicMock()

        with patch("kagan.ui.utils.clipboard.pyperclip.copy"):
            copy_with_notification(app, "test")

        assert "Content" in app.notify.call_args[0][0]
