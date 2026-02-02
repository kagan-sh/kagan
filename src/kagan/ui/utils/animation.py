"""Shared animation constants for consistent visual design."""

from __future__ import annotations

# Wave animation: cascading flip effect
# Sweeps left-to-right, then right-to-left
WAVE_FRAMES = [
    "ᘚᘚᘚᘚ",
    "ᘛᘚᘚᘚ",
    "ᘛᘛᘚᘚ",
    "ᘛᘛᘛᘚ",
    "ᘛᘛᘛᘛ",
    "ᘚᘛᘛᘛ",
    "ᘚᘚᘛᘛ",
    "ᘚᘚᘚᘛ",
]

# Animation timing (milliseconds per frame)
WAVE_INTERVAL_MS = 150
