#!/bin/sh
# cargo-mutants on the Rust lines changed since <base>. CI runs it; it is too heavy for a laptop.
# Fails when a changed line has a mutant no test notices. Deletes are not checked.
# Every package runs; the misses from all of them are listed once at the end.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

base=${1:?usage: sh scripts/mutants-diff.sh <base-ref>}
# CI splits the mutants across jobs; every shard must see the same diff and the same shard count.
shard_args=
if [ -n "${MUTANTS_SHARD:-}" ]; then
  shard_args="--shard $MUTANTS_SHARD"
fi
if ! git rev-parse --verify --quiet "$base^{commit}" > /dev/null; then
  echo "mutants-diff: base $base is not a commit here; fetch it first" >&2
  exit 1
fi
# A rewritten main shares no history with the old one; diff the trees then.
from=$(git merge-base "$base" HEAD) || from=$base

rust=$(git diff --name-only "$from" HEAD -- '*.rs')
if [ -z "$rust" ]; then
  echo "mutants-diff: no Rust changed since $from" >&2
  exit 0
fi

want=$(tr -d '[:space:]' < scripts/cargo-mutants-version)
have=$(cargo mutants --version 2>/dev/null | awk '{ print $2 }') || have=
if [ "$have" != "$want" ]; then
  echo "mutants-diff: need cargo-mutants $want, found ${have:-none}: cargo install cargo-mutants --version $want --locked" >&2
  exit 1
fi

# Timeouts mean a test hung on the mutant, so a test noticed it. Misses do not.
# Exit 3 wins over misses in cargo-mutants, so missed.txt is checked either way.
# --baseline skip: CI runs this after the tests passed on this tree, so there is
# no baseline to time; --timeout bounds a mutant that hangs a test.
# --in-place: CI's checkout is throwaway, and building there reuses its cached target/.
# It also means one mutant at a time; cargo-mutants refuses --jobs with it.
mutants() {
  manifest=$1
  pkg=$2
  diff=$3
  dir="target/mutants/diff/$pkg"
  mkdir -p "$dir"
  rc=0
  cargo mutants \
    --manifest-path "$manifest" \
    -p "$pkg" \
    --in-diff "$diff" \
    --baseline skip \
    --test-tool nextest \
    --cargo-arg=--locked \
    --output "$dir" \
    --in-place \
    --timeout 300 \
    $shard_args || rc=$?
  missed="$dir/mutants.out/missed.txt"
  if [ -s "$missed" ]; then
    sed "s/^/$pkg: /" "$missed" >> "$found"
  fi
  case $rc in
    0 | 3)
      if [ "$rc" -eq 3 ]; then
        echo "mutants-diff: $pkg mutant run timed out; treated as noticed" >&2
      fi
      ;;
    2)
      # Exit 2 with no missed list is an error; with one, the list says what was missed.
      if [ ! -s "$missed" ]; then
        echo "mutants-diff: cargo-mutants on $pkg exited 2" >&2
        exit 2
      fi
      ;;
    *)
      echo "mutants-diff: cargo-mutants on $pkg exited $rc" >&2
      exit "$rc"
      ;;
  esac
}

# A case pattern's ")" inside $(...) closes the substitution on macOS /bin/sh.
package_names() {
  printf '%s\n' "$rust" | while IFS= read -r path; do
    case $path in
      crates/*/*) ;;
      *) continue ;;
    esac
    dir=${path#crates/}
    dir=${dir%%/*}
    manifest="crates/$dir/Cargo.toml"
    [ -f "$manifest" ] || continue
    awk -F '"' '/^name = / { print $2; exit }' "$manifest"
  done | sort -u
}

pkgs=$(package_names)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if [ -n "$pkgs" ]; then
  git diff --no-ext-diff "$from" HEAD -- '*.rs' > "$work/root.diff"
  found="$work/missed"
  : > "$found"
  for pkg in $pkgs; do
    echo "mutants-diff: $pkg" >&2
    # A bare Cargo.toml has an empty parent, which cargo-mutants rejects.
    mutants ./Cargo.toml "$pkg" "$work/root.diff"
  done
  if [ -s "$found" ]; then
    echo "mutants-diff: changed lines no test caught:" >&2
    sed 's/^/  missed: /' "$found" >&2
    exit 1
  fi
fi
