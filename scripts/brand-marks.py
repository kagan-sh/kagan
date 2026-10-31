"""Write the kagan mark, wordmark and app icon SVGs.

The mark is the one path in assets/brand/kagan-mark-source.svg, a bold "KG"
monogram drawn y-up, as its editor exported it. The wordmark sets "kagan" in
Inter SemiBold (https://github.com/rsms/inter, SIL OFL 1.1), the bundled
assets/fonts/Inter-SemiBold.ttf. The SVGs hold outlines only, so they need no
font at run time.

Requires fontTools 4.67.0 (pip install fonttools==4.67.0).

    python3 -I scripts/brand-marks.py assets/brand/kagan-mark-source.svg assets/fonts/Inter-SemiBold.ttf assets/brand
"""

import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.recordingPen import RecordingPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.svgLib.path import parse_path
from fontTools.ttLib import TTFont

WORD = "kagan"
MARK = 256
MARK_INK = 232
HEIGHT = 243
GAP = HEIGHT / 3
BLACK = "#0E0E0E"
WHITE = "#FFFFFF"
FJORD_DARK = "#22262F"
EMBER_DARK = "#F0905F"
ICON = 1024
ICON_MARK = 560
ICON_RADIUS = 229


def number(value):
    return format(round(value, 2), "g")


def source(path):
    element = next(e for e in ET.parse(path).iter() if e.tag.endswith("path"))
    pen = RecordingPen()
    parse_path(element.get("d"), pen)
    box = BoundsPen(None)
    pen.replay(box)
    return pen, box.bounds


def place(mark, height, x, y):
    """Outline the mark `height` tall, its top-left at (x, y). Returns the path and width."""
    pen, (x0, y0, x1, y1) = mark
    scale = height / (y1 - y0)
    path = SVGPathPen(None, ntos=number)
    pen.replay(TransformPen(path, (scale, 0, 0, -scale, x - x0 * scale, y + y1 * scale)))
    return path.getCommands(), (x1 - x0) * scale


def run(font, text, scale, x, y):
    """Outline `text` at `scale`, its baseline origin at (x, y). Returns the path and the ink box."""
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    path = SVGPathPen(glyphs, ntos=number)
    box = BoundsPen(glyphs)
    at = x
    for char in text:
        name = cmap[ord(char)]
        move = (scale, 0, 0, -scale, at, y)
        glyphs[name].draw(TransformPen(path, move))
        glyphs[name].draw(TransformPen(box, move))
        at += font["hmtx"][name][0] * scale
    return path.getCommands(), box.bounds


def svg(width, height, body, title):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {number(width)} {number(height)}"'
        f' width="{number(width)}" height="{number(height)}"><title>{title}</title>{body}</svg>\n'
    )


def write(path, text):
    path.write_text(text, encoding="utf-8")
    print(path)


def main(source_path, inter_path, out):
    mark = source(source_path)
    inter = TTFont(inter_path)
    out = Path(out)

    _, width = place(mark, MARK_INK, 0, 0)
    square, _ = place(mark, MARK_INK, (MARK - width) / 2, (MARK - MARK_INK) / 2)
    for name, color in (("black", BLACK), ("white", WHITE)):
        body = f'<path fill="{color}" d="{square}"/>'
        write(out / f"kagan-mark-{name}.svg", svg(MARK, MARK, body, "kagan mark"))

    scale = HEIGHT / inter["OS/2"].sCapHeight
    _, (x0, top, _, _) = run(inter, WORD, scale, 0, 0)
    _, (_, _, _, bottom) = run(inter, WORD, scale, 0, -top)
    beside, mark_w = place(mark, HEIGHT, 0, -top - HEIGHT)
    word, (_, _, x1, _) = run(inter, WORD, scale, mark_w + GAP - x0, -top)
    for name, color in (("black", BLACK), ("white", WHITE)):
        body = f'<path fill="{color}" d="{beside}"/><path fill="{color}" d="{word}"/>'
        write(out / f"kagan-logo-{name}.svg", svg(x1, bottom, body, "kagan"))

    _, icon_w = place(mark, ICON_MARK, 0, 0)
    icon_mark, _ = place(mark, ICON_MARK, (ICON - icon_w) / 2, (ICON - ICON_MARK) / 2)
    background = f'<rect width="{ICON}" height="{ICON}" fill="{FJORD_DARK}"/>'
    ink = f'<path fill="{EMBER_DARK}" d="{icon_mark}"/>'
    rounded = (
        f'<rect width="{ICON}" height="{ICON}" rx="{ICON_RADIUS}" fill="{FJORD_DARK}"/>'
        f'<rect x="2" y="2" width="{ICON - 4}" height="{ICON - 4}" rx="{ICON_RADIUS - 2}"'
        ' fill="none" stroke="#FFFFFF" stroke-opacity=".08" stroke-width="4"/>'
    )
    write(out / "kagan-app-icon.svg", svg(ICON, ICON, rounded + ink, "kagan app icon"))
    write(
        out / "kagan-app-icon-full-bleed.svg",
        svg(ICON, ICON, background + ink, "kagan app icon, square"),
    )


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
