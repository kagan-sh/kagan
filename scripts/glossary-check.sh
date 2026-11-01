#!/bin/sh
# Banned domain synonyms (see docs/CONTEXT.md glossary-ban). Whole words, case-insensitive.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

context=docs/CONTEXT.md
ban=$(mktemp)
exc=$(mktemp)
hits=$(mktemp)
trap 'rm -f "$ban" "$exc" "$hits"' EXIT

awk '
  /^```glossary-ban/ { ban=1; next }
  /^```/ && ban { ban=0; next }
  ban && NF { print }
' "$context" > "$ban"

awk '
  /^```glossary-exceptions/ { exc=1; next }
  /^```/ && exc { exc=0; next }
  exc && /^#/ { next }
  exc && NF { print }
' "$context" > "$exc"

if [ ! -s "$ban" ]; then
  echo "glossary-check: missing glossary-ban block in $context" >&2
  exit 1
fi

while IFS= read -r line; do
  term=${line%%	*}
  [ -n "$term" ] || continue
  git grep -n -i -w -F -e "$term" -- 'crates/*/src/*' docs book \
    ':!docs/decisions.md' ':!docs/CONTEXT.md' | while IFS= read -r hit; do
    printf '%s\t%s\n' "$term" "$hit"
  done
done < "$ban" > "$hits"

if [ ! -s "$hits" ]; then
  echo "glossary-check: no banned terms in scope"
  exit 0
fi

python3 - "$exc" "$hits" <<'PY'
import sys

exc_path = sys.argv[1]
hits_path = sys.argv[2]

exceptions: list[tuple[str, str, str, str]] = []
with open(exc_path, encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        parts = line.split("\t")
        if len(parts) < 4:
            print(f"glossary-check: bad exception line: {line}", file=sys.stderr)
            sys.exit(1)
        exceptions.append(tuple(parts))

def allowed(term: str, path: str, text: str) -> bool:
    for t, path_sub, line_sub, _reason in exceptions:
        if t.lower() != term.lower():
            continue
        if path_sub != "*" and path_sub not in path:
            continue
        if line_sub != "*" and line_sub not in text:
            continue
        return True
    return False

failures: list[str] = []
with open(hits_path, encoding="utf-8") as f:
    for raw in f:
        raw = raw.rstrip("\n")
        if not raw:
            continue
        term, rest = raw.split("\t", 1)
        path, line_rest = rest.split(":", 1)
        line_no, text = line_rest.split(":", 1)
        if allowed(term, path, text):
            continue
        failures.append(f"{path}:{line_no}: {term!r} in {text}")

if failures:
    for item in failures:
        print(f"glossary-check: {item}", file=sys.stderr)
    sys.exit(1)

print("glossary-check: no banned terms in scope")
PY
