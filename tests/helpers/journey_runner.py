"""Journey runner for scripted Textual UI flows and snapshots."""

from __future__ import annotations

import io
import re
import xml.etree.ElementTree as ET
from typing import TYPE_CHECKING

from rich.console import Console

if TYPE_CHECKING:
    from collections.abc import Callable, Iterable
    from pathlib import Path

    from textual.app import App
    from textual.pilot import Pilot


REPEAT_PATTERN = re.compile(r"^(\w+)\((\d+)\)$")
WORD_PATTERN = re.compile(r"^\(([a-zA-Z]+)\)$")
SHOT_PATTERN = re.compile(r"^shot(?:\(([^()]+)\))?$")
WAIT_PATTERN = re.compile(r"^wait\((\d+(?:\.\d+)?)\)$")
WAIT_SCREEN_PATTERN = re.compile(r"^wait_screen\(([^()]+)\)$")


def parse_svg_styles(svg_content: str) -> tuple[dict[str, str], dict[str, str]]:
    """Parse SVG to extract CSS class definitions and text-to-class mappings."""
    root = ET.fromstring(svg_content)

    style_elem = None
    for elem in root.iter():
        if elem.tag.endswith("style"):
            style_elem = elem
            break
    css_text = style_elem.text if style_elem is not None and style_elem.text is not None else ""

    class_styles: dict[str, str] = {}
    for match in re.finditer(r"\\.([^\\s{]+)\\s*\\{([^}]+)\\}", css_text):
        class_name, properties = match.groups()
        class_styles[class_name] = properties.strip()

    text_to_class: dict[str, str] = {}
    for text_elem in root.iter():
        if text_elem.tag.endswith("text") or text_elem.tag == "text":
            class_attr = text_elem.get("class", "")
            text_content = "".join(text_elem.itertext()).replace("\\xa0", " ").strip()
            if text_content and class_attr:
                text_to_class[text_content] = class_attr

    return class_styles, text_to_class


def enrich_text_with_styles(
    plain_text: str, class_styles: dict[str, str], text_to_class: dict[str, str]
) -> str:
    """Append style information from SVG to plain text."""
    style_lines = ["", "--- STYLES ---"]
    for text, class_name in text_to_class.items():
        style_props = class_styles.get(class_name, "")
        if style_props:
            style_lines.append(f"- {text} .{class_name} {{ {style_props} }}")

    return plain_text + "\n".join(style_lines)


def export_text_screenshot(
    app: App, svg_content: str | None = None, export_styles: bool = False
) -> str:
    """Export a textual screenshot from the current app state."""
    width, height = app.size
    console = Console(
        width=width,
        height=height,
        file=io.StringIO(),
        force_terminal=True,
        color_system="truecolor",
        record=True,
        legacy_windows=False,
        safe_box=False,
    )
    screen_render = app.screen._compositor.render_update(
        full=True, screen_stack=app._background_screens, simplify=True
    )
    console.print(screen_render)
    plain_text = console.export_text(styles=False)

    if export_styles and svg_content:
        class_styles, text_to_class = parse_svg_styles(svg_content)
        return enrich_text_with_styles(plain_text, class_styles, text_to_class)
    return plain_text


def parse_actions(actions_str: str) -> list[str]:
    """Parse a space-delimited action string into a list of actions."""
    tokens = actions_str.split()
    result: list[str] = []
    for token in tokens:
        repeat_match = REPEAT_PATTERN.match(token)
        word_match = WORD_PATTERN.match(token)
        if repeat_match:
            action, count = repeat_match.groups()
            result.extend([action] * int(count))
        elif word_match:
            word = word_match.group(1)
            result.extend(list(word))
        else:
            result.append(token)
    return result


async def _wait_for_screen(pilot: Pilot, screen_type: type, timeout: float = 5.0) -> None:
    waited = 0.0
    while waited < timeout:
        for screen in pilot.app.screen_stack:
            if isinstance(screen, screen_type):
                return
        await pilot.pause(0.1)
        waited += 0.1
    raise AssertionError(f"Timed out waiting for screen {screen_type.__name__}")


async def execute_test_actions(
    pilot: Pilot,
    actions: Iterable[str],
    *,
    screen_registry: dict[str, type] | None = None,
    snapshot_dir: Path | None = None,
    export_text: bool = False,
) -> dict[str, str]:
    """Execute actions and capture named SVG snapshots.

    Returns a mapping of shot name -> svg content.
    """
    snapshots: dict[str, str] = {}
    await pilot.pause()
    for action in actions:
        shot_match = SHOT_PATTERN.match(action)
        wait_match = WAIT_PATTERN.match(action)
        wait_screen_match = WAIT_SCREEN_PATTERN.match(action)

        if shot_match:
            await pilot.pause()
            name = shot_match.group(1) or "screenshot"
            svg_content = pilot.app.export_screenshot()
            snapshots[name] = svg_content
            if snapshot_dir:
                snapshot_dir.mkdir(parents=True, exist_ok=True)
                txt_path = snapshot_dir / f"{name}.txt"
                svg_path = snapshot_dir / f"{name}.svg"
                if export_text:
                    txt_path.write_text(
                        export_text_screenshot(pilot.app, svg_content, export_styles=True)
                    )
                svg_path.write_text(svg_content)
        elif wait_match:
            await pilot.pause(float(wait_match.group(1)))
        elif wait_screen_match:
            if screen_registry is None:
                raise AssertionError("screen_registry is required for wait_screen(...) actions")
            screen_name = wait_screen_match.group(1)
            screen_type = screen_registry.get(screen_name)
            if screen_type is None:
                raise AssertionError(f"Unknown screen type: {screen_name}")
            await _wait_for_screen(pilot, screen_type)
        else:
            await pilot.press(action)
        await pilot.pause()

    return snapshots


def bundle_snapshots(
    snapshots: dict[str, str],
    *,
    normalizer: Callable[[str], str] | None = None,
) -> str:
    """Bundle multiple SVG snapshots into a single text snapshot."""
    parts: list[str] = []
    for name, svg in snapshots.items():
        content = normalizer(svg) if normalizer else svg
        content = content.replace("\r\n", "\n").rstrip()
        parts.append(f"--- SNAPSHOT: {name} ---\n{content}")
    return "\n\n".join(parts)
