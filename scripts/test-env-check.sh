#!/bin/sh
# A test reads a KG_* or KAGAN_* variable only when the same test sets it: a child
# process it starts itself, or a value it saves and restores. Each name below was
# reviewed for one of those patterns.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

exec python3 - "$@" <<'PY'
import pathlib
import re
import sys

REVIEWED = {
    "KG_TEST_DEFAULTS": "a test starts itself again as a child with this set",
    "KG_TEST_ID_HINT": "a test starts itself again as a child with this set",
    "KAGAN_FETCH_CASE": "a test starts itself again as a child with this set",
    "KAGAN_FETCH_SOURCE": "a test starts itself again as a child with this set",
    "KAGAN_LOCALE_TEST": "a test starts itself again as a child with this set",
    "KG_SOCKET": "saved before a test sets it, then restored",
    "KG_UPDATE_CLI": "the developer's switch that rewrites book/src/cli.md",
}

read = re.compile(r'var(?:_os)?\(\s*"((?:KG|KAGAN)_[A-Z0-9_]*)"')
errors: list[str] = []


def is_test_file(path: pathlib.Path) -> bool:
    return (
        "tests" in path.parts
        or path.name == "tests.rs"
        or path.name.endswith("_test.rs")
    )


for path in sorted(pathlib.Path("crates").rglob("*.rs")):
    if "target" in path.parts or not is_test_file(path):
        continue
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        for match in read.finditer(line):
            name = match.group(1)
            if name not in REVIEWED:
                errors.append(
                    f"test-env-check: {path}:{number}: a test reads {name}, "
                    "which it did not set. Set it in the test, or review it and add it to REVIEWED."
                )

if errors:
    for line in errors:
        print(line, file=sys.stderr)
    sys.exit(1)

print(f"test-env-check: tests read only variables they set ({len(REVIEWED)} reviewed names)")
PY
