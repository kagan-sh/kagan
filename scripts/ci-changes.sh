#!/bin/sh
# Classify a diff the way CI does. One path per line on stdin.
# Prints `skip` when no path can change what the Rust jobs build, test or check.
# A path not listed here runs the Rust jobs.
set -eu

skip=1
any=0
while IFS= read -r path; do
  [ -n "$path" ] || continue
  any=1
  case $path in
    crates/*) skip=0 ;;
    # Read by the Rust code or by the hygiene job.
    docs/CONTEXT.md | docs/architecture.md | docs/behaviour.md | docs/plugin-api.md | \
      docs/customize.md | docs/self-extend.md | docs/ui-driver.md | docs/themes/* | book/src/cli.md | book/src/start.md) skip=0 ;;
    *.md | docs/* | book/* | LICENSE | .githooks/* | .github/ISSUE_TEMPLATE/* | .github/dependabot.yml | \
      .github/workflows/docs.yml | .github/workflows/pr-title.yml | .github/workflows/release.yml | \
      scripts/brand-icons.sh | scripts/brand-marks.py | scripts/release-version.sh | scripts/typos-version) ;;
    *) skip=0 ;;
  esac
done

if [ "$any" -eq 1 ] && [ "$skip" -eq 1 ]; then
  echo skip
fi
