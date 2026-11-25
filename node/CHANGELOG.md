# Changelog

All notable changes to this node. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/) once a release is published.

## [Unreleased]

### Added

- Protocol v1: signed, content-addressed targets, submissions and attestations; canonical JSON; Ed25519; a hash-chained local ledger; pull-only HTTP sync in ledger order.
- Gates: `shape`, `no_sorry`, `kernel` (`lake comparator --paranoid`: Lean's kernel plus every bundled external kernel), `not_trivial` (claims must elaborate, and must not close with `omega`, `decide`, `simp` or `grind`), `not_duplicate`.
- Hardening:
  - Untrusted Lean runs only in bubblewrap.
  - Judging uses only operator-allowed, pre-installed toolchains, called directly, with no download.
  - Only comparator exit codes 0 and 1 are signed; anything else is "not judged".
  - Untrusted output is kept out of signed objects.
- Verifier tooling:
  - `materialize` judges a submission with plain Lean.
  - `reproduce` re-judges someone else's attestation and compares.
  - `vectors` prints the test vectors, which `scripts/check-vectors.py` checks independently.
- Docs: PROTOCOL, VERIFYING, THREAT-MODEL, SECURITY, CONTRIBUTING.
- Tests:
  - unit tests;
  - a two-node network test with stand-ins;
  - a real-Lean adversarial test with an independent verifier.
