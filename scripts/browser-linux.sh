#!/bin/sh
# Opens the Browser on Linux, under Xvfb when there is no display, and loads a local page.
# A non-Linux machine skips: WebKitGTK is the Linux page.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

if [ "$(uname -s)" != Linux ]; then
  echo "browser-linux: skipped on $(uname -s)"
  exit 0
fi

if [ -z "${DISPLAY:-}" ]; then
  exec xvfb-run -a -s "-screen 0 1280x800x24" sh "$0" "$@"
fi

binary=${1:-target/debug/kg}
if [ ! -x "$binary" ]; then
  echo "browser-linux: building $binary" >&2
  cargo build -p kg --locked
fi

tmp=$(mktemp -d)
page=$tmp/page
mkdir -p "$page" "$tmp/proj"
printf '%s\n' '<!doctype html><title>kagan-linux-page</title><p>kagan linux page</p>' >"$page/index.html"
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
python3 -m http.server --bind 127.0.0.1 "$port" --directory "$page" >"$tmp/http.log" 2>&1 &
http=$!

KG_DATA_DIR=$tmp/data
KG_SOCKET=$tmp/kg.sock
KG_NO_START=1
export KG_DATA_DIR KG_SOCKET KG_NO_START
log=$tmp/app.log
snapshot=$tmp/snapshot.json
app=

alive() {
  state=$(ps -p "$app" -o stat= 2>/dev/null | tr -d '[:space:]')
  [ -n "$state" ] || return 1
  case $state in
  Z*) return 1 ;;
  esac
  return 0
}

cleanup() {
  if [ -n "$app" ] && alive; then
    "$binary" window quit >/dev/null 2>&1 || true
    sleep 1
    kill "$app" 2>/dev/null || true
  fi
  kill "$http" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
  echo "browser-linux: $1" >&2
  echo "browser-linux: kept $tmp" >&2
  if [ -f "$log" ]; then
    tail -40 "$log" >&2
  fi
  if [ -f "$snapshot" ]; then
    echo "browser-linux: snapshot:" >&2
    tail -c 4000 "$snapshot" >&2 || true
    echo >&2
  fi
  exit 1
}

"$binary" --headless >"$log" 2>&1 &
app=$!

i=0
while [ ! -S "$KG_SOCKET" ]; do
  alive || fail "the app exited before it made its socket"
  [ "$i" -lt 40 ] || fail "no socket within 40s"
  i=$((i + 1))
  sleep 1
done

"$binary" open >/dev/null || fail "kg open failed"

i=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null &&
    grep -Fq 'sidebar-new-thread' "$snapshot"; then
    break
  fi
  [ "$i" -lt 20 ] || fail "the window never showed New thread"
  i=$((i + 1))
  sleep 1
done

added=$("$binary" --json project add "$tmp/proj") || fail "project add failed"
project=$(printf '%s' "$added" | python3 -c 'import json,sys; print(json.load(sys.stdin)["project"]["id"])')
ran=$("$binary" --json thread run --project-id "$project" --program /bin/true --environment project-default) ||
  fail "thread run failed"
thread=$(printf '%s' "$ran" | python3 -c 'import json,sys; print(json.load(sys.stdin)["thread"]["id"])')

i=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null &&
    grep -Fq "thread-$thread" "$snapshot"; then
    break
  fi
  [ "$i" -lt 20 ] || fail "the new thread never appeared"
  i=$((i + 1))
  sleep 1
done
"$binary" ui click "thread-$thread" >/dev/null || fail "could not show the thread"

i=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null &&
    grep -Fq 'right-tab-add' "$snapshot"; then
    break
  fi
  [ "$i" -lt 20 ] || fail "the right sidebar never showed Add tab"
  i=$((i + 1))
  sleep 1
done

"$binary" ui click right-tab-add >/dev/null || fail "could not open the add menu"
"$binary" ui click right-add-browser >/dev/null || fail "could not open the Browser"

i=0
while :; do
  "$binary" ui snapshot --json >"$snapshot" 2>/dev/null || true
  if grep -Fq 'Type an address and press Enter.' "$snapshot"; then
    break
  fi
  [ "$i" -lt 10 ] || fail "the Browser did not ask for an address"
  i=$((i + 1))
  sleep 1
done

"$binary" ui type "http://127.0.0.1:${port}/" >/dev/null || fail "could not type the address"
"$binary" ui key enter >/dev/null || fail "could not submit the address"

i=0
while :; do
  "$binary" ui snapshot --json >"$snapshot" 2>/dev/null || true
  if grep -Fq 'could not open' "$snapshot" || grep -Fq 'needs an X11 window' "$snapshot"; then
    fail "the page did not open"
  fi
  if ! grep -Fq 'Type an address and press Enter.' "$snapshot"; then
    break
  fi
  [ "$i" -lt 20 ] || fail "the address hint stayed, so no page was created"
  i=$((i + 1))
  sleep 1
done

i=0
while :; do
  if grep -Fq 'GET /' "$tmp/http.log"; then
    break
  fi
  "$binary" ui snapshot --json >"$snapshot" 2>/dev/null || true
  if grep -Fq 'could not open' "$snapshot" || grep -Fq 'needs an X11 window' "$snapshot"; then
    fail "the page did not load"
  fi
  [ "$i" -lt 20 ] || fail "the local page was never requested"
  i=$((i + 1))
  sleep 1
done

echo "browser-linux: ok"
