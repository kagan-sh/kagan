"""Shared Textual theme definition for Kagan."""

from __future__ import annotations

from textual.theme import Theme

# Khagan Night Theme - Inspired by the Mongol steppe and Silk Road
KAGAN_THEME = Theme(
    name="kagan",
    primary="#3fb58e",  # Jade Green - precious stones traded on Silk Road
    secondary="#d4a84b",  # Khan Gold - royal Mongol ornaments
    accent="#4ec9b0",  # Turquoise - Turkic jewelry
    foreground="#c5cdd9",  # Pale Silver - moonlit snow on mountains
    background="#0f1419",  # Deep Charcoal Blue - night sky over steppe
    surface="#171c24",  # Dark Slate - felt textures of a ger
    panel="#1e2530",  # Current Line - subtle lift
    warning="#e6c07b",  # Pale Amber - firelight in felt tents
    error="#e85535",  # Blood Orange - battle intensity
    success="#3fb58e",  # Jade Green
    dark=True,
    variables={
        "border": "#2a3342",  # Smoke Gray - borders
        "border-blurred": "#2a334280",
        "text-muted": "#5c6773",  # Steppe Dust - comments
        "text-disabled": "#5c677380",
        "input-cursor-foreground": "#0f1419",
        "input-cursor-background": "#d4a84b",  # Gold beacon cursor
        "input-selection-background": "#3fb58e33",  # Jade with transparency
        "scrollbar": "#2a3342",
        "scrollbar-hover": "#3fb58e",
        "scrollbar-active": "#d4a84b",
        "link-color": "#6fa3d4",  # Tengri Blue
        "link-hover-color": "#4ec9b0",
        "footer-key-foreground": "#5c6773",  # text-muted (subtle)
        "footer-key-background": "transparent",  # no background
        "footer-description-foreground": "#5c677380",  # text-disabled
        "button-foreground": "#c5cdd9",
        "button-color-foreground": "#0f1419",
    },
)
