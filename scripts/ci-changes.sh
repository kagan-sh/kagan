#!/bin/sh
# Classify a diff the way CI does. One path per line on stdin.
# Prints `skip` when every path is markdown or under docs/ or book/.
set -eu

skip=1
any=0
while IFS= read -r path; do
  [ -n "$path" ] || continue
  any=1
  case $path in
    *.md | docs/* | book/*) ;;
    *) skip=0 ;;
  esac
done

if [ "$any" -eq 1 ] && [ "$skip" -eq 1 ]; then
  echo skip
fi
