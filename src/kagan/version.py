"""Shared package version helpers."""

from __future__ import annotations

from functools import lru_cache
from importlib.metadata import PackageNotFoundError, version


@lru_cache(maxsize=1)
def get_kagan_version() -> str:
    """Return installed kagan version, or 'dev' when package metadata is unavailable."""
    try:
        return version("kagan")
    except PackageNotFoundError:
        return "dev"


__all__ = ["get_kagan_version"]
