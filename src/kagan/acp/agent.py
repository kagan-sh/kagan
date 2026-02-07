"""Agent module - re-exports KaganAgent for convenience."""

from __future__ import annotations

from kagan.acp.kagan_agent import KaganAgent

Agent = KaganAgent

__all__ = ["Agent", "KaganAgent"]
