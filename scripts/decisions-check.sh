#!/bin/sh
# docs/decisions.md numbers go up in file order with no repeats and no gaps, except
# RETIRED. Every "decision N" and "Superseded by N" in the repo names a real entry.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

exec python3 - "$@" <<'PY'
import pathlib
import re
import sys

# Numbers removed before this check existed. They are never reused, and nothing may cite them.
RETIRED = {85, 126, 150, 151, 152, 153}
SCAN = ["AGENTS.md", "docs", "book/src", "crates", "scripts", ".githooks", ".github"]
SUFFIXES = {".md", ".rs", ".sh", ".py", ".toml", ".yml", ".yaml", ".json", ".txt"}
MAX_RANGE = 50

errors: list[str] = []


def fail(path: pathlib.Path, line: int, message: str) -> None:
    errors.append(f"decisions-check: {path}:{line}: {message}")


decisions_path = pathlib.Path("docs/decisions.md")
decisions_text = decisions_path.read_text(encoding="utf-8")
entry_re = re.compile(r"^\*\*(\d+)\.(?:\*\*| )", re.MULTILINE)
entries = [int(m.group(1)) for m in entry_re.finditer(decisions_text)]

if len(entries) != len(set(entries)):
    dupes = sorted({n for n in entries if entries.count(n) > 1})
    fail(decisions_path, 1, f"repeated numbers: {dupes}")

if entries != sorted(entries):
    fail(decisions_path, 1, "entries are not in number order")

present = set(entries)
for n in sorted(RETIRED & present):
    fail(decisions_path, 1, f"retired number {n} is back in use")
expected = set(range(1, max(entries) + 1)) - RETIRED
missing = sorted(expected - present)
if missing:
    fail(decisions_path, 1, f"gaps in numbering: {missing}")

blocks = list(entry_re.finditer(decisions_text))
for i, match in enumerate(blocks):
    end = blocks[i + 1].start() if i + 1 < len(blocks) else len(decisions_text)
    block = decisions_text[match.start():end]
    line = decisions_text[: match.start()].count("\n") + 1
    for sup in re.finditer(r"Superseded by \d+", block):
        if sup.start() > 200:
            fail(decisions_path, line, f"entry {match.group(1)}: 'Superseded by' must come right after the title")

placeholder = re.compile(r"\bNNN\b")
number_list = re.compile(
    r"\b[Dd]ecisions? (\d+(?:\s*(?:to|-|–)\s*\d+)?(?:(?:,\s*(?:and\s+|or\s+)?|\s+(?:and|or)\s+)\d+(?:\s*(?:to|-|–)\s*\d+)?)*)"
)
superseded = re.compile(r"[Ss]uperseded by (\d+)")
item = re.compile(r"(\d+)(?:\s*(?:to|-|–)\s*(\d+))?")


def check_number(path: pathlib.Path, line: int, n: int) -> None:
    if n in RETIRED:
        fail(path, line, f"cites retired decision {n}")
    elif n not in present:
        fail(path, line, f"cites decision {n}, which does not exist")


roots = [pathlib.Path(p) for p in SCAN]
for root in roots:
    files = [root] if root.is_file() else sorted(root.rglob("*")) if root.is_dir() else []
    for path in files:
        if not path.is_file() or path.suffix not in SUFFIXES:
            continue
        if "target" in path.parts or path.name == "decisions-check.sh":
            continue
        for number, line_text in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            if placeholder.search(line_text):
                fail(path, number, "NNN placeholder left in a decision reference")
            for ref in number_list.finditer(line_text):
                for item_match in item.finditer(ref.group(1)):
                    low = int(item_match.group(1))
                    high = int(item_match.group(2)) if item_match.group(2) else low
                    if high - low > MAX_RANGE:
                        fail(path, number, f"range {low}-{high} is too wide to check")
                        continue
                    for n in range(low, high + 1):
                        check_number(path, number, n)
            for sup in superseded.finditer(line_text):
                check_number(path, number, int(sup.group(1)))

if errors:
    for line in errors:
        print(line, file=sys.stderr)
    sys.exit(1)

print(f"decisions-check: {len(present)} entries, numbers and references agree")
PY
