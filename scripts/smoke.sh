#!/bin/sh
# `--strict` needs macOS: Linux has no `kg ui screenshot`.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

usage() {
  echo "usage: sh scripts/smoke.sh [--binary <path>] [--keep] [--strict]"
}

binary=target/debug/kg
keep=0
strict=0
while [ $# -gt 0 ]; do
  case $1 in
    --binary)
      [ $# -ge 2 ] || { usage >&2; exit 2; }
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
      usage >&2
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

# Own data dir and socket, so the caller's app is left alone; the path must fit sun_path
# (103 bytes on macOS), and a mktemp folder does.
tmp=$(mktemp -d) || { echo "smoke: cannot make a temp folder" >&2; exit 1; }
KG_DATA_DIR=$tmp/data
KG_SOCKET=$tmp/kg.sock
KG_NO_START=1
export KG_DATA_DIR KG_SOCKET KG_NO_START
log=$tmp/app.log
snapshot=$tmp/snapshot.json
shot=$tmp/smoke.png
app=

# A zombie is not a running app; `kill -0` cannot tell them apart.
alive() {
  state=$(ps -p "$app" -o stat= 2>/dev/null | tr -d '[:space:]')
  [ -n "$state" ] || return 1
  case $state in
  Z*) return 1 ;;
  esac
  return 0
}

cleanup() {
  [ -n "$app" ] || return 0
  if alive; then
    for child in $(ps -ax -o pid= -o ppid= | awk -v parent="$app" '$2 == parent { print $1 }'); do
      kill "$child" 2>/dev/null || true
    done
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

# A client call can start its own app, so readiness is the socket file, not `kg status`.
i=0
while [ ! -S "$KG_SOCKET" ]; do
  alive || fail "$binary exited before it made its socket"
  [ "$i" -lt 30 ] || fail "$binary made no socket within 30s"
  i=$((i + 1))
  sleep 1
done
alive || fail "the app exited before answering kg status"

"$binary" status >/dev/null || fail "kg status failed"
"$binary" open >/dev/null || fail "kg open failed"
alive || fail "the app exited after kg open"

i=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null &&
    grep -Fq '"id":"Name(\"sidebar-new-thread\")"' "$snapshot"; then
    break
  fi
  [ "$i" -lt 15 ] || fail "the window never showed the sidebar's New thread control; see $snapshot"
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

alive || fail "the app exited before the end of the run"

"$binary" window quit >/dev/null || fail "kg window quit failed"

i=0
while alive; do
  i=$((i + 1))
  if [ "$i" -ge 15 ]; then
    fail "the app did not quit within 15s"
  fi
  sleep 1
done
app=

echo "smoke: ok"
echo "smoke: snapshot $snapshot"
if [ -s "$shot" ]; then
  echo "smoke: screenshot $shot"
else
  echo "smoke: screenshot missing (see the warning above)"
fi
if [ "$keep" -eq 1 ]; then
  echo "smoke: kept $tmp"
else
  rm -rf "$tmp"
fi
