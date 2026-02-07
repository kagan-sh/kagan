"""UI utilities package."""

from __future__ import annotations

from kagan.ui.utils.clipboard import copy_with_notification
from kagan.ui.utils.queries import safe_query_one

__all__ = ["copy_with_notification", "safe_query_one"]
