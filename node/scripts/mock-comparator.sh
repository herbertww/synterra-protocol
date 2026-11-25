#!/usr/bin/env bash
# TEST ONLY. NOT A PROOF CHECKER.
#
# Stands in for `lake comparator` so the node's pipeline (signing, storage,
# sync, gates, attestations) can be exercised on machines without Lean.
# It only looks at text: the solution module exists, every target theorem
# name is declared in it, and it contains no `sorry`. It proves nothing.
# Never point a real node at this script.
set -euo pipefail
config="$1"
solution=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["solution_module"].replace(".","/")+".lean")' "$config")
[ -f "$solution" ] || { echo "mock: missing $solution"; exit 1; }
if grep -qw sorry "$solution"; then echo "mock: sorry found"; exit 1; fi
for t in $(python3 -c 'import json,sys; print(" ".join(json.load(open(sys.argv[1]))["theorem_names"]))' "$config"); do
  grep -Eq "^(theorem|lemma) $t\b" "$solution" || { echo "mock: $t not declared"; exit 1; }
done
echo "mock: text checks passed (NOT a kernel check)"
