#!/bin/sh
# CRAP report for product Rust under crates/ (plugin JS out of scope).
# Prints functions scoring above 30, highest first. Report only: it never blocks a PR.
# Exits non-zero only when coverage or crapper fails, because then there is no report.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

rev=$(tr -d '[:space:]' < scripts/crapper-rev)
dir=target/crapper-checkout
if [ ! -d "$dir/.git" ]; then
  git clone https://github.com/unclebob/crapper "$dir"
fi
(
  cd "$dir"
  git fetch --depth 1 origin "$rev"
  git checkout --force FETCH_HEAD
)

if [ ! -x "$dir/.venv/bin/python" ]; then
  rm -rf "$dir/.venv"
  if command -v uv >/dev/null 2>&1; then
    uv venv "$dir/.venv"
    uv pip install --python "$dir/.venv/bin/python" -q -e "$dir"
  else
    python3 -m venv "$dir/.venv"
    "$dir/.venv/bin/pip" install -q -e "$dir"
  fi
fi

lcov=target/coverage/rust/lcov.info
mkdir -p target/coverage/rust

# CI writes this from one nextest run. Generate it here when that did not happen.
if [ ! -s "$lcov" ]; then
  export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
  cargo llvm-cov --workspace --locked --lcov --output-path "$lcov"
fi

files=$(mktemp)
scores=$(mktemp)
trap 'rm -f "$files" "$scores"' EXIT

find crates -type f -name '*.rs' | awk -F/ '
  $1 == "crates" {
    name = $NF
    if (name == "tests.rs" || name ~ /_test\.rs$/) next
    for (i = 2; i < NF; i++) if ($i == "tests") next
    print
  }
' | sort > "$files"

xargs "$dir/.venv/bin/python" -m crapper --use-existing-coverage < "$files" >/dev/null

if [ ! -f .metrics/crap.edn ]; then
  echo "crap-check: crapper wrote no .metrics/crap.edn" >&2
  exit 1
fi

awk '
/:name "/ {
  line = $0
  if (line !~ /:crap /) next
  name = line
  sub(/^.*:name "/, "", name)
  sub(/".*$/, "", name)
  ns = line
  sub(/^.*:namespace "/, "", ns)
  sub(/".*$/, "", ns)
  crap = line
  sub(/^.*:crap /, "", crap)
  sub(/}.*$/, "", crap)
  crap = crap + 0
  if (crap > 30) {
    printf "%.1f\t%s::%s\n", crap, ns, name
  }
}
' .metrics/crap.edn | sort -u | sort -rn > "$scores"

cat "$scores"
echo "crap-check: $(wc -l < "$scores" | tr -d ' ') product functions above 30 (report only, does not block)"
