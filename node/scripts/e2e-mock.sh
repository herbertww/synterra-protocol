#!/usr/bin/env bash
# Two-node network test with TEST-ONLY stand-ins for Lean (no Lean needed).
# Node A nominates a target and submits; node B pulls everything from A,
# checks what is pending and signs attestations; A pulls them back.
set -euo pipefail
BIN=${BIN:-target/release/synterra}
HERE=$(cd "$(dirname "$0")/.." && pwd)
T=$(mktemp -d)
A="$T/a"; B="$T/b"
trap 'kill $(jobs -p) 2>/dev/null || true; rm -rf "$T"' EXIT

# A fake elan home so the node finds "the toolchain"; the stand-ins never run it.
export ELAN_HOME="$T/elan"; FAKE_TC="$ELAN_HOME/toolchains/leanprover--lean4---v4.36.0/bin"
mkdir -p "$FAKE_TC" && touch "$FAKE_TC/lake" "$FAKE_TC/lean"

$BIN --datadir "$A" init >/dev/null
$BIN --datadir "$B" init >/dev/null
cat > "$B/synterra.toml" <<EOF
listen = "127.0.0.1:18591"
peers = ["http://127.0.0.1:18590"]
sync_secs = 5

[checker]
command = ["bash", "$HERE/scripts/mock-comparator.sh", "{config}"]
sandbox = ["bash", "$HERE/scripts/mock-sandbox.sh"]
timeout_secs = 60
trivial_tactics = ["omega", "decide", "(simp; done)", "grind"]
EOF
# A keeps and relays only: no checker configured, so it never judges.
cat > "$A/synterra.toml" <<EOF
listen = "127.0.0.1:18590"
peers = ["http://127.0.0.1:18591"]
sync_secs = 5

[checker]
command = []
sandbox = []
timeout_secs = 60
trivial_tactics = []
EOF

# The target must carry lake-manifest.json; a minimal one is enough for the stand-ins.
PROJ="$T/gauss"; cp -r "$HERE/examples/gauss" "$PROJ"
[ -f "$PROJ/lake-manifest.json" ] || printf '{"version": "1.1.0", "packagesDir": ".lake/packages", "packages": [], "name": "gauss", "lakeDir": ".lake"}\n' > "$PROJ/lake-manifest.json"

S="$HERE/examples/solutions"
TID=$($BIN --datadir "$A" nominate "$PROJ" --name "Gauss sum" --theorem gauss --theorem gauss_succ --baseline-loc 12)
echo "target      $TID"
GOOD=$($BIN --datadir "$A" submit "$TID" "$S/good/Solution.lean" \
  --lemma 'two_mul_sumTo::(n : Nat) : 2 * sumTo n = n * (n + 1)' --note "name the pairing")
CHEAT=$($BIN --datadir "$A" submit "$TID" "$S/cheat/Solution.lean" --note "cheat")
DUP=$($BIN --datadir "$A" submit "$TID" "$S/good/Solution.lean" \
  --lemma 'two_mul_sumTo::(n : Nat) :   2 * sumTo n = n * (n + 1)' --note "same lemma again")
TRIV=$($BIN --datadir "$A" submit "$TID" "$S/good/Solution.lean" --lemma 'zero_add_easy::(a : Nat) : 0 + a = a' --note "trivial")
echo "good        $GOOD"; echo "cheat       $CHEAT"; echo "duplicate   $DUP"; echo "trivial     $TRIV"

# Tampering: a peer may not alter a signed object.
$BIN --datadir "$A" show "$GOOD" | sed 's/name the pairing/name the pairinG/' > "$T/forged.json"
if $BIN --datadir "$B" import "$T/forged.json" 2>/dev/null; then echo "FAIL: forged object accepted"; exit 1; fi
echo "forgery     rejected"

$BIN --datadir "$A" run >"$T/a.log" 2>&1 &
sleep 1
$BIN --datadir "$B" run >"$T/b.log" 2>&1 &
sleep 14

echo "--- node B log"; cat "$T/b.log"
echo "--- attestations on A (pulled back from B)"
$BIN --datadir "$A" list attestations
$BIN --datadir "$A" verify
$BIN --datadir "$B" verify

verdict() { $BIN --datadir "$A" list attestations | grep "submission ${1:0:12}" | awk '{print $4}'; }
[ "$(verdict "$GOOD")" = accepted ]  || { echo "FAIL: good not accepted"; exit 1; }
[ "$(verdict "$CHEAT")" = rejected ] || { echo "FAIL: cheat not rejected"; exit 1; }
[ "$(verdict "$DUP")" = rejected ]   || { echo "FAIL: duplicate lemma not rejected"; exit 1; }
[ "$(verdict "$TRIV")" = rejected ]  || { echo "FAIL: trivial lemma not rejected"; exit 1; }
echo "E2E MOCK OK"
