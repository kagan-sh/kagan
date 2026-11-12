#!/bin/sh
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

lock=$(shasum -a 256 Cargo.lock | awk 'NR==1 { print $1 }')
crates=$(mktemp)
trap 'rm -f "$crates"' EXIT

cargo about generate --offline --locked --fail -c .config/about.toml \
  --manifest-path crates/kg/Cargo.toml \
  .config/about.hbs --output-file "$crates"

out=assets/THIRD-PARTY-NOTICES.txt
# quickjs-jit-stdlib vendors LLRT under Apache-2.0, so its NOTICE, which carries LLRT's, ships too.
stdlib=$(cargo metadata --format-version 1 --locked --offline --manifest-path crates/kg/Cargo.toml |
  python3 -c 'import json, sys; print(next(p["manifest_path"] for p in json.load(sys.stdin)["packages"] if p["name"] == "quickjs-jit-stdlib"))')
notice=$(dirname "$stdlib")/NOTICE
[ -f "$notice" ] || { echo "notices: no NOTICE beside $stdlib" >&2; exit 1; }
{
  echo "Third-party notices for the kg binary."
  echo "cargo-lock $lock"
  echo
  echo "kagan itself is Apache-2.0. See LICENSE."
  echo
  echo "The window, the terminal, and the wordmark use these fonts. Each is under the SIL Open Font License 1.1."
  echo
  echo "--------------------------------------------------------------------------------"
  echo "Inter"
  echo "--------------------------------------------------------------------------------"
  cat assets/fonts/Inter-OFL.txt
  echo
  echo "--------------------------------------------------------------------------------"
  echo "JetBrains Mono"
  echo "--------------------------------------------------------------------------------"
  cat assets/fonts/JetBrainsMono-OFL.txt
  echo
  echo "--------------------------------------------------------------------------------"
  echo "Agent marks in the program picker"
  echo "--------------------------------------------------------------------------------"
  cat assets/agents/README.md
  echo
  echo "--- Primer Octicons (copilot.svg) ---"
  cat assets/agents/Primer-MIT.txt
  echo
  echo "--- Simple Icons (CC0 1.0; see assets/agents/README.md for each mark's owner) ---"
  echo "--------------------------------------------------------------------------------"
  echo "NOTICE of quickjs-jit-stdlib, which includes LLRT's"
  echo "--------------------------------------------------------------------------------"
  cat "$notice"
  echo
  echo "Crate licenses follow. Each block is the license text and the crates that use it."
  echo
  cat "$crates"
} > "$out"

python3 - "$out" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text()
needles = [
    "SIL Open Font License",
    "Inter",
    "JetBrains Mono",
    "Copyright Amazon.com, Inc. or its affiliates.",
    "Apache License",
    "Permission is hereby granted",
    "THE SOFTWARE IS PROVIDED",
]
missing = [needle for needle in needles if needle not in text]
if missing or "&quot;" in text or len(text) < 100_000:
    raise SystemExit(f"notices: incomplete ({missing or 'escaped or short'})")
print(f"notices: {path} {path.stat().st_size} bytes")
PY
