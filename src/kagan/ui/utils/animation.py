"""Shared animation constants and display helpers for consistent visual design."""

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
# Target: 10-15 FPS for CPU efficiency (66-100ms intervals)
# Current: 100ms = 10 FPS (previously was 150ms = 6.67 FPS)
WAVE_INTERVAL_MS = 100
