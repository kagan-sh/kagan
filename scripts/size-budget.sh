#!/bin/sh
# Product Rust line ceilings. They must match the current counts.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

budget=.config/size-budget.txt
if [ ! -f "$budget" ]; then
  echo "size-budget: missing $budget" >&2
  exit 1
fi

files=$(mktemp)
counts=$(mktemp)
fails=$(mktemp)
trap 'rm -f "$files" "$counts" "$fails" "$markers"' EXIT

find crates -type f -name '*.rs' | awk -F/ '
  $1 == "crates" {
    name = $NF
    if (name == "tests.rs" || name ~ /_test\.rs$/) next
    for (i = 2; i < NF; i++) if ($i == "tests" || $i == "target") next
    print
  }
' | sort > "$files"

while IFS= read -r path; do
  [ -n "$path" ] || continue
  lines=$(wc -l < "$path" | tr -d '[:space:]')
  printf '%s %s\n' "$path" "$lines"
done < "$files" > "$counts"

markers=$(mktemp)
marker_total=0
while IFS= read -r path; do
  [ -n "$path" ] || continue
  n=$(grep -cE '#!?\[(allow|expect)\(' "$path") || [ $? -eq 1 ]
  marker_total=$((marker_total + n))
done < "$files"
printf '%s\n' "$marker_total" > "$markers"

awk -v counts="$counts" -v markers="$markers" -v budget="$budget" -v fails="$fails" '
function bad(msg) {
  print msg >> fails
  failed = 1
}
function num(s) {
  return s ~ /^[0-9]+$/
}
BEGIN {
  total = 0
  failed = 0
  while ((getline line < counts) > 0) {
    split(line, f, " ")
    count[f[1]] = f[2]
    total += f[2]
  }
  close(counts)
  if ((getline marker_count < markers) <= 0 || !num(marker_count))
    bad("size-budget: missing marker count from " markers)
  close(markers)
}
/^#/ || NF == 0 { next }
$1 == "target" { target = $2; next }
$1 == "total" { ceiling = $2; next }
$1 == "allow_expect" { marker_ceiling = $2; next }
NF == 2 && num($2) { cap[$1] = $2; next }
{ bad("size-budget: bad line in " budget ": " $0) }
END {
  if (!num(target))
    bad("size-budget: " budget " needs a \"target <lines>\" line")
  if (!num(ceiling))
    bad("size-budget: " budget " needs a \"total <lines>\" ceiling")
  else if (total + 0 > ceiling + 0)
    bad("size-budget: product Rust is " total " lines, above the ceiling of " ceiling)
  else if (total + 0 < ceiling + 0)
    bad("size-budget: product Rust is " total " lines, under the ceiling of " ceiling "; set total to " total " in " budget)

  if (!num(marker_ceiling))
    bad("size-budget: " budget " needs an \"allow_expect <count>\" ceiling")
  else if (marker_count + 0 > marker_ceiling + 0)
    bad("size-budget: product Rust has " marker_count " #[allow]/#[expect] markers, above the ceiling of " marker_ceiling)
  else if (marker_count + 0 < marker_ceiling + 0)
    bad("size-budget: product Rust has " marker_count " #[allow]/#[expect] markers, under the ceiling of " marker_ceiling "; set allow_expect to " marker_count " in " budget)

  for (path in count) {
    n = count[path] + 0
    if (path in cap) {
      c = cap[path] + 0
      if (n > c)
        bad("size-budget: " path " is " n " lines, above its ceiling of " c)
      else if (n <= 600)
        bad("size-budget: " path " is " n " lines; remove it from " budget)
      else if (n < c)
        bad("size-budget: " path " is " n " lines, under its ceiling of " c "; set it to " n)
    } else if (n > 600)
      bad("size-budget: " path " is " n " lines; unlisted product files must stay at 600 or fewer")
  }
  for (path in cap)
    if (!(path in count))
      bad("size-budget: " path " is listed in " budget " but is not a product file; remove it")

  if (!failed && num(ceiling) && total + 0 == ceiling + 0)
    print "size-budget: " total " product lines, within the ceiling of " ceiling
}
' "$budget"

if [ -s "$fails" ]; then
  sort "$fails" >&2
  exit 1
fi
