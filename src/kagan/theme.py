"""Shared Textual theme definition for Kagan."""

from __future__ import annotations

from textual.theme import Theme

# Khagan Night Theme - Inspired by the Mongol steppe and Silk Road
# This is the full truecolor (24-bit) theme for modern terminals
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

# Khagan Night Theme - 256-color fallback for terminals without truecolor
# Hand-picked colors from the xterm-256 palette for best visual fidelity
# Use this theme when supports_truecolor() returns False
KAGAN_THEME_256 = Theme(
    name="kagan-256",
    # Colors use xterm-256 palette indices mapped to their hex equivalents
    primary="#5faf87",  # color(72) - closest to Jade Green
    secondary="#d7af5f",  # color(179) - closest to Khan Gold
    accent="#5fd7af",  # color(79) - closest to Turquoise
    foreground="#d0d0d0",  # color(252) - closest to Pale Silver
    background="#121212",  # color(233) - closest to Deep Charcoal Blue
    surface="#1c1c1c",  # color(234) - closest to Dark Slate
    panel="#262626",  # color(235) - closest to Current Line
    warning="#d7af87",  # color(180) - closest to Pale Amber
    error="#d75f5f",  # color(167) - closest to Blood Orange
    success="#5faf87",  # color(72) - same as primary
    dark=True,
    variables={
        "border": "#303030",  # color(236) - closest to Smoke Gray
        "border-blurred": "#30303080",
        "text-muted": "#6c6c6c",  # color(242) - closest to Steppe Dust
        "text-disabled": "#6c6c6c80",
        "input-cursor-foreground": "#121212",  # same as background
        "input-cursor-background": "#d7af5f",  # same as secondary
        "input-selection-background": "#5faf8733",  # primary with transparency
        "scrollbar": "#303030",  # same as border
        "scrollbar-hover": "#5faf87",  # same as primary
        "scrollbar-active": "#d7af5f",  # same as secondary
        "link-color": "#5fafd7",  # color(74) - closest to Tengri Blue
        "link-hover-color": "#5fd7af",  # same as accent
        "footer-key-foreground": "#6c6c6c",  # same as text-muted
        "footer-key-background": "transparent",
        "footer-description-foreground": "#6c6c6c80",  # same as text-disabled
        "button-foreground": "#d0d0d0",  # same as foreground
        "button-color-foreground": "#121212",  # same as background
    },
)
