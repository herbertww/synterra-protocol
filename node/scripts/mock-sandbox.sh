#!/usr/bin/env bash
# TEST ONLY. NOT A SANDBOX AND NOT LEAN.
#
# Stands in for "bwrap ... lake ..." in the triviality probes so the network
# test runs on machines without Lean. It pretends the challenge builds, every
# claim elaborates, and only the claim "0 + a = a" is closed by a stock tactic.
set -euo pipefail
last="${*: -1}"
case "$last" in
  SynterraTrivial.lean) grep -q "0 + a = a" SynterraTrivial.lean && exit 0 || exit 1 ;;
  *) exit 0 ;;
esac
