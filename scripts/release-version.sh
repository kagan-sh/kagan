#!/bin/bash
set -eu

check_version() {
  python3 - "$@" <<'PY'
import re
import subprocess
import sys

pattern = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?\Z")


def parse(value):
    match = pattern.fullmatch(value)
    if not match:
        return None
    pre = match.group(4)
    parts = pre.split('.') if pre else []
    if any(part.isdigit() and len(part) > 1 and part[0] == '0' for part in parts):
        return None
    return (int(match.group(1)), int(match.group(2)), int(match.group(3)),
            (0, tuple((0, int(part)) if part.isdigit() else (1, part) for part in parts)) if pre else (1, ()))


version = sys.argv[1].removeprefix('v')
parsed = parse(version)
if parsed is None:
    sys.exit('Enter a valid semantic version, such as 0.2.0 or 0.2.0-rc.1.')
tags = subprocess.check_output(['git', 'tag', '--list', 'v*'], text=True).splitlines()
tag = 'v' + version
if tag in tags:
    sys.exit(f'Tag {tag} already exists.')
previous = [(parsed_tag, name) for name in tags if (parsed_tag := parse(name[1:])) is not None]
last = max(previous, default=None)
if last and parsed <= last[0]:
    sys.exit(f'Version {version} must be higher than {last[1]}.')
print(f'tag={tag}')
print(f'version={version}')
print(f'previous={last[1] if last else ""}')
print(f'prerelease={str("-" in version).lower()}')
PY
}

case "${1:-}" in
--test)
  dir=$(mktemp -d)
  trap 'rm -rf "$dir"' EXIT
  git -C "$dir" init -q
  git -C "$dir" -c user.name=test -c user.email=test@example.com commit -q --allow-empty -m test
  cd "$dir"
  git tag v1.2.3
  git tag v1.1.0
  for version in 1.2.2 1.2.3 1.1.0 01.3.0 1.3.0-01 1.3; do
    if check_version "$version" >/dev/null 2>&1; then
      echo "Expected $version to fail." >&2
      exit 1
    fi
  done
  check_version v1.3.0-rc.1 | grep -qx 'prerelease=true'
  git tag v1.3.0-rc.2
  if check_version 1.3.0-rc.1 >/dev/null 2>&1; then
    echo "Expected an older prerelease to fail." >&2
    exit 1
  fi
  check_version 1.3.0 | grep -qx 'previous=v1.3.0-rc.2'
  echo 'Release version checks passed.'
  ;;
check)
  check_version "${2:-}"
  ;;
*)
  echo 'Usage: release-version.sh check <version> | --test' >&2
  exit 2
  ;;
esac
