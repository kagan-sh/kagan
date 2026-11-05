#!/bin/sh
# Live UI smoke gate: an isolated data dir, a real window, one snapshot, one screenshot, clean quit.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

usage() {
  echo "usage: sh scripts/smoke.sh [--binary <path>] [--keep] [--strict]" >&2
}

binary=target/debug/kg
keep=0
strict=0
while [ $# -gt 0 ]; do
  case $1 in
    --binary)
      [ $# -ge 2 ] || { usage; exit 2; }
      binary=$2
      shift 2
      ;;
    --keep)
      keep=1
      shift
      ;;
    --strict)
      strict=1
      shift
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      echo "smoke: unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [ ! -x "$binary" ]; then
  if [ "$binary" != target/debug/kg ]; then
    echo "smoke: $binary is missing or not executable" >&2
    exit 1
  fi
  echo "smoke: building $binary" >&2
  if ! cargo build -p kg --locked; then
    echo "smoke: cargo build -p kg --locked failed" >&2
    exit 1
  fi
  [ -x "$binary" ] || { echo "smoke: $binary is missing after the build" >&2; exit 1; }
fi

# The caller's app is left alone: a data dir brings its own socket (decision 124).
unset KG_SOCKET
tmp=$(mktemp -d) || { echo "smoke: cannot make a temp folder" >&2; exit 1; }
KG_DATA_DIR=$tmp/data
export KG_DATA_DIR
log=$tmp/app.log
snapshot=$tmp/snapshot.json
shot=$tmp/smoke.png
app=

cleanup() {
  if [ -n "$app" ] && kill -0 "$app" 2>/dev/null; then
    kill "$app" 2>/dev/null || true
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke: $1" >&2
  echo "smoke: kept $tmp" >&2
  echo "smoke: log $log" >&2
  if [ -f "$log" ]; then
    tail -40 "$log" >&2
  fi
  exit 1
}

"$binary" --headless >"$log" 2>&1 &
app=$!

# The app binds its socket before it opens the workspace. A client that arrives first
# would start a second app, so give the bind a moment.
sleep 1

i=0
ready=0
while [ "$i" -lt 30 ]; do
  if kill -0 "$app" 2>/dev/null && "$binary" status --json >/dev/null 2>&1; then
    ready=1
    break
  fi
  kill -0 "$app" 2>/dev/null || fail "$binary exited before answering kg status"
  i=$((i + 1))
  sleep 1
done
[ "$ready" -eq 1 ] || fail "$binary did not answer kg status within 30s"

"$binary" open >/dev/null || fail "kg open failed"

i=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null &&
    grep -q '"label":"New thread"' "$snapshot"; then
    break
  fi
  [ "$i" -lt 15 ] || fail "the window never showed a New thread control; see $snapshot"
  i=$((i + 1))
  sleep 1
done

if "$binary" ui screenshot "$shot" --json >"$tmp/capture.json" 2>&1; then
  [ -s "$shot" ] || fail "ui screenshot wrote no image to $shot"
  if grep -q '"current":false' "$tmp/capture.json"; then
    if [ "$strict" -eq 1 ]; then
      fail "the window had not drawn a new frame before the screenshot"
    fi
    echo "smoke: warning: the window had not drawn a new frame; the image may show an earlier state" >&2
  fi
else
  if [ "$strict" -eq 1 ]; then
    fail "ui screenshot failed: $(tr -d '\n' <"$tmp/capture.json")"
  fi
  echo "smoke: warning: ui screenshot failed: $(tr -d '\n' <"$tmp/capture.json")" >&2
fi

"$binary" window quit >/dev/null || fail "kg window quit failed"

i=0
while kill -0 "$app" 2>/dev/null; do
  i=$((i + 1))
  if [ "$i" -ge 15 ]; then
    fail "the app did not quit within 15s"
  fi
  sleep 1
done
app=

echo "smoke: ok"
echo "smoke: snapshot $snapshot"
echo "smoke: screenshot $shot"
if [ "$keep" -eq 1 ]; then
  echo "smoke: kept $tmp"
else
  rm -rf "$tmp"
fi
