#!/bin/sh
# Colors come from the Workbench theme tokens (docs/look.md), never literals in views.
set -eu

cd "$(CDPATH='' cd -- "$(dirname "$0")/.." && pwd)"
root=${1:-.}

exec python3 - "$root" <<'PY'
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])

HEX = re.compile(r"#([0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b")
CTOR = re.compile(r"(?<!\.)\b(?:rgb|rgba|hsl|hsla)\s*\(")
# Six or eight hex digits, underscores between digits, optional Rust integer suffix.
INT = re.compile(
    r"\b0x[0-9a-fA-F](?:_?[0-9a-fA-F]){5}(?:(?:_?[0-9a-fA-F]){2})?"
    r"(?:_?(?:[ui](?:8|16|32|64|128|size)))?\b",
    re.IGNORECASE,
)

def product_rs(directory: Path):
    if not directory.is_dir():
        return
    for path in sorted(directory.rglob("*.rs")):
        name = path.name
        if name == "tests.rs" or name.endswith("_test.rs"):
            continue
        if "tests" in path.parts:
            continue
        yield path

def line_hits(text: str):
    hits = []
    for i, line in enumerate(text.splitlines(), 1):
        if HEX.search(line) or CTOR.search(line) or INT.search(line):
            hits.append((i, line.strip()))
    return hits

errors = []

ui = root / "crates" / "ui" / "src"
for path in product_rs(ui):
    if path.name == "theme.rs" or path.parent.name == "theme":
        continue
    text = path.read_text(encoding="utf-8")
    for lineno, line in line_hits(text):
        rel = path.relative_to(root)
        errors.append(
            f"view-colors: {rel}:{lineno}: color literal in a view. "
            "Colors come from the Workbench theme tokens (docs/look.md), "
            "never literals in views.\n"
            f"    {line}"
        )

if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)

print("view-colors: views use theme tokens")
PY
