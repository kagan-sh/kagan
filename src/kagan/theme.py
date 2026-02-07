"""Shared Textual theme definition for Kagan."""

from __future__ import annotations

from textual.theme import Theme

KAGAN_THEME = Theme(
    name="kagan",
    primary="#3fb58e",
    secondary="#d4a84b",
    accent="#4ec9b0",
    foreground="#c5cdd9",
    background="#0f1419",
    surface="#171c24",
    panel="#1e2530",
    warning="#e6c07b",
    error="#e85535",
    success="#3fb58e",
    dark=True,
    variables={
        "border": "#2a3342",
        "border-blurred": "#2a334280",
        "text-muted": "#5c6773",
        "text-disabled": "#5c677380",
        "input-cursor-foreground": "#0f1419",
        "input-cursor-background": "#d4a84b",
        "input-selection-background": "#3fb58e33",
        "scrollbar": "#2a3342",
        "scrollbar-hover": "#3fb58e",
        "scrollbar-active": "#d4a84b",
        "link-color": "#6fa3d4",
        "link-hover-color": "#4ec9b0",
        "footer-key-foreground": "#5c6773",
        "footer-key-background": "transparent",
        "footer-description-foreground": "#5c677380",
        "button-foreground": "#c5cdd9",
        "button-color-foreground": "#0f1419",
    },
)


KAGAN_THEME_256 = Theme(
    name="kagan-256",
    primary="#5faf87",
    secondary="#d7af5f",
    accent="#5fd7af",
    foreground="#d0d0d0",
    background="#121212",
    surface="#1c1c1c",
    panel="#262626",
    warning="#d7af87",
    error="#d75f5f",
    success="#5faf87",
    dark=True,
    variables={
        "border": "#303030",
        "border-blurred": "#30303080",
        "text-muted": "#6c6c6c",
        "text-disabled": "#6c6c6c80",
        "input-cursor-foreground": "#121212",
        "input-cursor-background": "#d7af5f",
        "input-selection-background": "#5faf8733",
        "scrollbar": "#303030",
        "scrollbar-hover": "#5faf87",
        "scrollbar-active": "#d7af5f",
        "link-color": "#5fafd7",
        "link-hover-color": "#5fd7af",
        "footer-key-foreground": "#6c6c6c",
        "footer-key-background": "transparent",
        "footer-description-foreground": "#6c6c6c80",
        "button-foreground": "#d0d0d0",
        "button-color-foreground": "#121212",
    },
)
