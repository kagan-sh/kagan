"""Safe widget query utilities."""

from __future__ import annotations

from contextlib import suppress
from typing import TypeVar

from textual.css.query import NoMatches
from textual.widget import Widget

T = TypeVar("T", bound=Widget)


def safe_query_one(
    parent: Widget,
    selector: str,
    widget_class: type[T],
    default: T | None = None,
) -> T | None:
    """Query widget safely, returning default on NoMatches."""
    with suppress(NoMatches):
        return parent.query_one(selector, widget_class)
    return default
