#!/bin/sh
# Workspace crate graph must match docs/architecture.md (normal dependencies only).
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

exec python3 - "$@" <<'PY'
import json
import subprocess
import sys

GPUI = frozenset(
    {
        "gpui-kit",
        "gpui-shell",
        "gpui-component-shell",
        "gpui-pre",
    }
)

# Direct kg-* edges from docs/architecture.md
EXPECTED = {
    "kg": {"kg-core", "kg-domain", "kg-plugins", "kg-process", "kg-ui"},
    "kg-ui": {"kg-core", "kg-domain", "kg-plugins"},
    "kg-plugins": {"kg-core", "kg-domain", "kg-process"},
    "kg-core": {"kg-domain", "kg-process", "kg-store", "kg-terminal"},
    "kg-store": {"kg-domain"},
    "kg-domain": set(),
    "kg-process": set(),
    "kg-terminal": {"kg-process"},
}

WORKSPACE = set(EXPECTED)

raw = subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--locked"],
    text=True,
)
meta = json.loads(raw)

id_to_name: dict[str, str] = {}
for pkg in meta["packages"]:
    id_to_name[pkg["id"]] = pkg["name"]

resolve = meta.get("resolve") or {}
nodes = {node["id"]: node for node in resolve.get("nodes", [])}

def normal_dep_names(pkg_name: str) -> set[str]:
    pkg = next(p for p in meta["packages"] if p["name"] == pkg_name)
    out: set[str] = set()
    for dep in pkg.get("dependencies", []):
        if dep.get("kind") not in (None, "normal"):
            continue
        dep_name = dep["name"]
        if dep_name in WORKSPACE:
            out.add(dep_name)
    return out

def gpui_reachable(from_name: str) -> list[str]:
    start = next(p["id"] for p in meta["packages"] if p["name"] == from_name)
    seen: set[str] = set()
    stack = [start]
    hit: list[str] = []
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        name = id_to_name.get(pid, "")
        if name in GPUI:
            hit.append(name)
            continue
        node = nodes.get(pid)
        if not node:
            continue
        for dep in node.get("dependencies", []):
            stack.append(dep)
    return sorted(set(hit))

errors: list[str] = []

gpui_hits = gpui_reachable("kg-domain")
if gpui_hits:
    errors.append(
        "kg-domain must not depend on GPUI crates; reachable: "
        + ", ".join(gpui_hits)
    )

for pkg in meta["packages"]:
    name = pkg["name"]
    if name not in WORKSPACE:
        continue
    for dep in pkg.get("dependencies", []):
        if dep.get("kind") not in (None, "normal"):
            continue
        dep_name = dep["name"]
        if dep_name == "kg-store" and name != "kg-core":
            errors.append(f"only kg-core may depend on {dep_name}; {name} does too")

for crate, want in sorted(EXPECTED.items()):
    have = normal_dep_names(crate)
    if have != want:
        extra = sorted(have - want)
        missing = sorted(want - have)
        parts = [f"crate graph mismatch for {crate}"]
        if extra:
            parts.append(f"unexpected edges: {', '.join(extra)}")
        if missing:
            parts.append(f"missing edges: {', '.join(missing)}")
        errors.append("; ".join(parts))

if errors:
    for line in errors:
        print(f"crate-boundaries: {line}", file=sys.stderr)
    sys.exit(1)

print("crate-boundaries: workspace graph matches the EXPECTED table in this script")
PY
