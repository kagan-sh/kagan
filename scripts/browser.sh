#!/bin/sh
# Opens the Browser, loads a local page, drives it, and closes the tab.
# Linux with no display re-execs under Xvfb. The page color is checked from a
# window capture on Windows, and on macOS when the system allows a capture.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

os=$(uname -s)
case $os in
Linux)
  if [ -z "${DISPLAY:-}" ]; then
    # Xvfb has no DRI3 device. WebKitGTK's GPU process then never issues the request.
    LIBGL_ALWAYS_SOFTWARE=1
    WEBKIT_DISABLE_COMPOSITING_MODE=1
    export LIBGL_ALWAYS_SOFTWARE WEBKIT_DISABLE_COMPOSITING_MODE
    exec xvfb-run -a -s "-screen 0 1280x800x24" sh "$0" "$@"
  fi
  pixels=skip
  ;;
Darwin)
  pixels=optional
  ;;
MINGW* | MSYS* | CYGWIN*)
  pixels=required
  ;;
*)
  echo "browser: skipped on $os"
  exit 0
  ;;
esac

# Ubuntu blocks the user namespaces WebKit's sandbox needs, so the page never starts.
if [ "$os" = Linux ] &&
  [ -r /proc/sys/kernel/apparmor_restrict_unprivileged_userns ] &&
  [ "$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns)" != 0 ]; then
  sudo -n sysctl -w kernel.apparmor_restrict_unprivileged_userns=0 ||
    echo "browser: could not allow user namespaces; WebKit's sandbox may not start" >&2
fi

binary=${1:-}
if [ -z "$binary" ]; then
  if [ -x target/debug/kg.exe ]; then
    binary=target/debug/kg.exe
  else
    binary=target/debug/kg
  fi
fi
if [ ! -x "$binary" ]; then
  echo "browser: building $binary" >&2
  cargo build -p kg --locked
fi

py=$(command -v python3 || command -v python || true)
[ -n "$py" ] || {
  echo "browser: python is not installed" >&2
  exit 1
}

tmp=$(mktemp -d)
page=$tmp/page
mkdir -p "$page" "$tmp/proj"
cat >"$page/index.html" <<'EOF'
<!doctype html>
<title>kagan-browser-page</title>
<style>
  html, body { margin: 0; background: #ff0080; }
</style>
<p id="out">kagan browser page</p>
<label>Name <input id="name"></label>
<button type="button" id="go">Mark</button>
<input type="file" aria-label="Upload">
<script>
document.getElementById("go").addEventListener("click", function () {
  alert("stay");
  var name = document.getElementById("name").value;
  document.getElementById("out").textContent = "marked " + name;
});
</script>
EOF
port=$("$py" -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
"$py" -m http.server --bind 127.0.0.1 "$port" --directory "$page" >"$tmp/http.log" 2>&1 &
http=$!

# Windows names the pipe from the data directory. A unix path in KG_SOCKET is not a pipe name.
KG_DATA_DIR=$tmp/data
if command -v cygpath >/dev/null 2>&1; then
  KG_DATA_DIR=$(cygpath -w "$tmp/data")
else
  KG_SOCKET=$tmp/kg.sock
  export KG_SOCKET
fi
KG_NO_START=1
export KG_DATA_DIR KG_NO_START
log=$tmp/app.log
snapshot=$tmp/snapshot.json
app=
saw_paint=0

alive() {
  if ps -p "$app" -o stat= >/dev/null 2>&1; then
    state=$(ps -p "$app" -o stat= 2>/dev/null | tr -d '[:space:]')
    [ -n "$state" ] || return 1
    case $state in
    Z*) return 1 ;;
    esac
    return 0
  fi
  kill -0 "$app" 2>/dev/null
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
  echo "browser: $1" >&2
  echo "browser: kept $tmp" >&2
  if [ -f "$log" ]; then
    tail -40 "$log" >&2
  fi
  if [ -f "$snapshot" ]; then
    echo "browser: snapshot:" >&2
    tail -c 4000 "$snapshot" >&2 || true
    echo >&2
  fi
  exit 1
}

paint() {
  goal=$1
  i=0
  misses=0
  count=0
  while :; do
    alive || fail "the app exited during the capture"
    if "$binary" ui screenshot "$tmp/page.png" --json >"$tmp/capture.json" 2>"$tmp/capture.err"; then
      count=$("$py" scripts/browser-pixels.py "$tmp/page.png") || fail "could not read the capture"
      if [ "$goal" = present ] && [ "$count" -ge 200 ]; then
        saw_paint=1
        return 0
      fi
      if [ "$goal" = gone ] && [ "$count" -le 40 ]; then
        return 0
      fi
    else
      misses=$((misses + 1))
      if [ "$pixels" = optional ] && [ "$saw_paint" -eq 0 ] && [ "$misses" -ge 8 ]; then
        echo "browser: capture unavailable; the page color was not checked" >&2
        cat "$tmp/capture.err" >&2 || true
        pixels=skip
        return 0
      fi
    fi
    if [ "$i" -ge 15 ]; then
      if [ "$misses" -gt 0 ] && [ "$count" -eq 0 ]; then
        fail "could not capture the window: $(cat "$tmp/capture.err" 2>/dev/null || true)"
      fi
      if [ "$goal" = present ]; then
        fail "the page did not paint ($count pixels of its color)"
      fi
      fail "the page stayed on screen after its tab closed ($count pixels of its color)"
    fi
    i=$((i + 1))
    sleep 1
  done
}

"$binary" --headless >"$log" 2>&1 &
app=$!

i=0
while ! "$binary" status >/dev/null 2>&1; do
  alive || fail "the app exited before it listened"
  [ "$i" -lt 40 ] || fail "the app did not listen within 40s"
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
  alive || fail "the app exited before the window opened"
  [ "$i" -lt 20 ] || fail "the window never showed New thread"
  i=$((i + 1))
  sleep 1
done

added=$("$binary" --json project add "$tmp/proj") || fail "project add failed"
project=$(printf '%s' "$added" | "$py" -c 'import json,sys; print(json.load(sys.stdin)["project"]["id"])')
# Git's true.exe lives under "Program Files", and the program line splits on that space.
program=/usr/bin/true
case $os in
MINGW* | MSYS* | CYGWIN*) program="cmd /c exit 0" ;;
esac
ran=$("$binary" --json thread run --project-id "$project" --program "$program" --environment project-default) ||
  fail "thread run failed"
thread=$(printf '%s' "$ran" | "$py" -c 'import json,sys; print(json.load(sys.stdin)["thread"]["id"])')

i=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null &&
    grep -Fq "thread-$thread" "$snapshot"; then
    break
  fi
  alive || fail "the app exited before the thread appeared"
  [ "$i" -lt 20 ] || fail "the new thread never appeared"
  i=$((i + 1))
  sleep 1
done
"$binary" ui click "thread-$thread" >/dev/null || fail "could not show the thread"

# A narrow screen hides the right column. Open it over the center once.
i=0
opened=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null &&
    grep -Fq 'right-tab-add' "$snapshot"; then
    break
  fi
  alive || fail "the app exited before the sidebar opened"
  if [ "$opened" -eq 0 ] && [ "$i" -ge 2 ]; then
    "$binary" ui click right-sidebar-toggle >/dev/null 2>&1 || true
    opened=1
  fi
  [ "$i" -lt 20 ] || fail "the right sidebar never showed Add tab"
  i=$((i + 1))
  sleep 1
done

"$binary" ui click right-tab-add >/dev/null || fail "could not open the add menu"
"$binary" ui click right-add-browser >/dev/null || fail "could not open the Browser"

i=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null &&
    grep -Fq 'Type an address and press Enter.' "$snapshot"; then
    break
  fi
  alive || fail "the app exited while opening the Browser"
  [ "$i" -lt 10 ] || fail "the Browser did not ask for an address"
  i=$((i + 1))
  sleep 1
done

"$binary" ui type "http://127.0.0.1:${port}/" >/dev/null || fail "could not type the address"
"$binary" ui key enter >/dev/null || fail "could not submit the address"

i=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null; then
    if grep -Fq 'could not open' "$snapshot" || grep -Fq 'needs an X11 window' "$snapshot"; then
      fail "the page did not open"
    fi
    if ! grep -Fq 'Type an address and press Enter.' "$snapshot"; then
      break
    fi
  fi
  alive || fail "the app exited while opening the page"
  [ "$i" -lt 20 ] || fail "the address hint stayed, so no page was created"
  i=$((i + 1))
  sleep 1
done

i=0
while :; do
  if grep -Fq 'GET /' "$tmp/http.log"; then
    break
  fi
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null; then
    if grep -Fq 'could not open' "$snapshot" || grep -Fq 'needs an X11 window' "$snapshot"; then
      fail "the page did not load"
    fi
    if ! grep -Fq 'right-tab-browser' "$snapshot"; then
      fail "the Browser tab closed before the page loaded"
    fi
  fi
  alive || fail "the app exited while the page was loading"
  [ "$i" -lt 30 ] || fail "the local page was never requested"
  i=$((i + 1))
  sleep 1
done

if KG_THREAD_ID=$thread "$binary" browser open 'file:///etc/passwd' >"$tmp/file-url.out" 2>"$tmp/file-url.err"; then
  fail "a file URL opened in the browser"
fi
if ! grep -Fq 'web address' "$tmp/file-url.err"; then
  fail "a file URL was not refused"
fi

if ! KG_THREAD_ID=$thread "$binary" --json browser open "http://127.0.0.1:${port}/" \
  >"$tmp/driven.json" 2>"$tmp/driven.err"; then
  echo "browser: $(cat "$tmp/driven.err")" >&2
  fail "kg browser open failed"
fi
if ! grep -Fq 'kagan-browser-page' "$tmp/driven.json"; then
  fail "kg browser open did not report the page title"
fi

i=0
while :; do
  if KG_THREAD_ID=$thread "$binary" --json browser snapshot >"$tmp/snap.json" 2>/dev/null &&
    grep -Fq '"name":"Mark"' "$tmp/snap.json" &&
    grep -Fq '"name":"Upload"' "$tmp/snap.json"; then
    break
  fi
  alive || fail "the app exited while reading the page"
  [ "$i" -lt 20 ] || fail "the page snapshot never listed Mark and Upload"
  i=$((i + 1))
  sleep 1
done

refs=$("$py" - "$tmp/snap.json" <<'PY'
import json, sys
snap = json.load(open(sys.argv[1]))["browser-snapshot"]
want = {("textbox", "Name"): "name", ("button", "Mark"): "mark", ("file", "Upload"): "upload"}
found = {}
for control in snap["controls"]:
    key = (control["role"], control["name"])
    if key in want:
        found[want[key]] = control["ref"]
missing = [name for name in want.values() if name not in found]
if missing:
    raise SystemExit("missing " + ", ".join(missing))
print(found["name"], found["mark"], found["upload"])
PY
) || fail "could not read the snapshot refs"
set -- $refs
name=$1
mark=$2
upload=$3

if KG_THREAD_ID=$thread "$binary" browser click "$upload" >"$tmp/upload.out" 2>"$tmp/upload.err"; then
  fail "a file input was accepted"
fi
if ! grep -Fq 'cannot choose a file' "$tmp/upload.err"; then
  fail "a file input did not say why"
fi

KG_THREAD_ID=$thread "$binary" browser fill "$name" ada >/dev/null || fail "could not fill the name"
KG_THREAD_ID=$thread "$binary" browser click "$mark" >/dev/null || fail "could not click Mark; a script dialog may have stalled it"

i=0
while :; do
  if KG_THREAD_ID=$thread "$binary" --json browser snapshot >"$tmp/after.json" 2>/dev/null &&
    grep -Fq 'marked ada' "$tmp/after.json"; then
    break
  fi
  alive || fail "the app exited after the click"
  [ "$i" -lt 10 ] || fail "the click did not mark the page"
  i=$((i + 1))
  sleep 1
done

if [ "$pixels" != skip ]; then
  paint present
fi

# A host that does not exist never commits, so the page has no URL. Reading it
# used to panic while drawing the address and abort the app.
if ! KG_THREAD_ID=$thread "$binary" browser goto 'https://kagan-missing.invalid/' \
  >"$tmp/missing.out" 2>"$tmp/missing.err"; then
  alive || fail "a missing website crashed kagan"
  fail "a missing website was not opened: $(cat "$tmp/missing.err" 2>/dev/null || true)"
fi
alive || fail "a missing website crashed kagan"
"$binary" ui snapshot --json >"$snapshot" || fail "the window did not answer after a missing website"
grep -Fq 'right-tab-browser' "$snapshot" || fail "a missing website closed the Browser"

"$binary" ui click right-tab-close-browser >/dev/null || fail "could not close the Browser"
alive || fail "the app exited when the Browser closed"

i=0
while :; do
  if "$binary" ui snapshot --json >"$snapshot" 2>/dev/null &&
    ! grep -Fq 'right-tab-browser' "$snapshot"; then
    break
  fi
  alive || fail "the app exited when the Browser closed"
  [ "$i" -lt 15 ] || fail "closing the Browser left the tab up"
  i=$((i + 1))
  sleep 1
done

if KG_THREAD_ID=$thread "$binary" browser snapshot >"$tmp/closed.json" 2>"$tmp/closed.err"; then
  fail "the page still answered after the tab closed"
fi

if [ "$saw_paint" -eq 1 ]; then
  paint gone
fi

alive || fail "the app exited after the Browser closed"
echo "browser: ok"
