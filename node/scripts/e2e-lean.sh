#!/usr/bin/env bash
# Real checking: Lean >= 4.35 (elan) and bubblewrap on PATH. Linux or WSL2 only.
# Node A nominates the Gauss target and judges six submissions with
# `lake comparator --paranoid`. Node V, an independent verifier with its own
# key, then reproduces A's attestations and judges one submission by hand.
set -euo pipefail
BIN=${BIN:-target/release/synterra}
HERE=$(cd "$(dirname "$0")/.." && pwd)
T=$(mktemp -d)
A="$T/a"; V="$T/v"
trap 'rm -rf "$T"' EXIT

# The target must carry lake-manifest.json; build it once (trusted: it is ours).
PROJ="$T/gauss"; cp -r "$HERE/examples/gauss" "$PROJ"
(cd "$PROJ" && lake build >/dev/null)

$BIN --datadir "$A" init >/dev/null
TID=$($BIN --datadir "$A" nominate "$PROJ" --name "Gauss sum" --theorem gauss --theorem gauss_succ --baseline-loc 12)
echo "target $TID"

S="$HERE/examples/solutions"
GOOD=$($BIN --datadir "$A" submit "$TID" "$S/good/Solution.lean" --lemma 'two_mul_sumTo::(n : Nat) : 2 * sumTo n = n * (n + 1)')
WRONG=$($BIN --datadir "$A" submit "$TID" "$S/wrong-statement/Solution.lean")
TRIV=$($BIN --datadir "$A" submit "$TID" "$S/good/Solution.lean" --lemma 'zero_add_easy::(a : Nat) : 0 + a = a' --note "trivial claim")
BAD=$($BIN --datadir "$A" submit "$TID" "$S/good/Solution.lean" --lemma 'bogus::(a : Nat) : a + = 1' --note "malformed claim")
CHEAT=$($BIN --datadir "$A" submit "$TID" "$S/cheat/Solution.lean" --note "sorry")
export SYNTERRA_TEST_KEY="$A/node.key"
HOSTILE=$($BIN --datadir "$A" submit "$TID" "$S/hostile/Solution.lean" --note "tries to read the key")

run() { echo; echo "### $1"; $BIN --datadir "$A" check "$2" || true; }
run "good: proves both through one named lemma (expect accepted)" "$GOOD"
run "wrong statements under the targets' names (expect rejected by kernel)" "$WRONG"
run "claims a lemma a stock tactic closes (expect rejected by not_trivial)" "$TRIV"
run "claims a statement that does not elaborate (expect rejected by not_trivial)" "$BAD"
run "sorry (expect rejected by no_sorry)" "$CHEAT"
run "hostile: reads the node key at build time (expect rejected, key not leaked)" "$HOSTILE"

KEY=$(cat "$A/node.key")
if grep -rq "$KEY" "$A/objects"; then echo "FAIL: node key appears in a stored object"; exit 1; fi
echo; echo "key not present in any stored object"
echo "--- hostile log excerpt"; grep -iE "error|STOLEN|denied|No such" "$A/logs/$HOSTILE.log" | head -4 || true
$BIN --datadir "$A" verify

verdict() { $BIN --datadir "$A" list attestations | grep "submission ${1:0:12}" | awk '{print $4}'; }
[ "$(verdict "$GOOD")" = accepted ]    || { echo "FAIL: good not accepted"; exit 1; }
[ "$(verdict "$WRONG")" = rejected ]   || { echo "FAIL: wrong statement not rejected"; exit 1; }
[ "$(verdict "$TRIV")" = rejected ]    || { echo "FAIL: trivial lemma not rejected"; exit 1; }
[ "$(verdict "$BAD")" = rejected ]     || { echo "FAIL: malformed claim not rejected"; exit 1; }
[ "$(verdict "$CHEAT")" = rejected ]   || { echo "FAIL: sorry not rejected"; exit 1; }
[ "$(verdict "$HOSTILE")" = rejected ] || { echo "FAIL: hostile submission not rejected"; exit 1; }

# --- An independent verifier: own key, own store, imports A's objects, reproduces.
echo; echo "### independent verifier reproduces A's attestations"
$BIN --datadir "$V" init >/dev/null
for id in $($BIN --datadir "$A" list targets | awk '{print $1}') \
          $($BIN --datadir "$A" list submissions | awk '{print $1}') \
          $($BIN --datadir "$A" list attestations | awk '{print $1}'); do
  $BIN --datadir "$A" show "$id" > "$T/obj.json"; $BIN --datadir "$V" import "$T/obj.json" >/dev/null
done
ATT_GOOD=$($BIN --datadir "$A" list attestations | grep "submission ${GOOD:0:12}" | awk '{print $1}')
ATT_WRONG=$($BIN --datadir "$A" list attestations | grep "submission ${WRONG:0:12}" | awk '{print $1}')
$BIN --datadir "$V" reproduce "$ATT_GOOD"
$BIN --datadir "$V" reproduce "$ATT_WRONG"

echo; echo "### judged by hand, with plain Lean and no trust in this binary"
$BIN --datadir "$V" materialize "$GOOD" "$T/byhand"
(cd "$T/byhand" && lake comparator --config=comparator.json --paranoid >/dev/null 2>&1) \
  && echo "lake comparator --paranoid: accepted" || { echo "FAIL: hand check rejected the good submission"; exit 1; }
echo "E2E LEAN OK"
