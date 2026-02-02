"""Enum coercion utilities."""

from __future__ import annotations

from enum import Enum
from typing import TypeVar

T = TypeVar("T", bound=Enum)


def coerce_enum(value: T | str | int, enum_class: type[T]) -> T:
    """Safely coerce value to enum type."""
    if isinstance(value, enum_class):
        return value
    return enum_class(value)
