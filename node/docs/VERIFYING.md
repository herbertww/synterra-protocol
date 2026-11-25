# Verifying us

You should not have to trust this software, or us. This page lists every way to check our work, from "trust nothing we wrote" downwards. Each step needs less of your time and more of your trust.

## 1. Judge a submission with plain Lean (trust none of our code)

`materialize` writes a submission and its target as an ordinary Lean project, with the exact comparator configuration our node uses:

```bash
synterra materialize <submission-id> ./check-it
cd check-it
cat SYNTERRA.json          # what is being judged, and the claimed lemmas
lake comparator --config=comparator.json --paranoid
```

Exit code 0 means accepted, 1 means rejected. This is Lean's own judge, built into Lean since v4.35. `--paranoid` replays the proof through Lean's kernel and every external kernel bundled with Lean: leanchecker-paranoid, lean4lean, nanoda, con-leche and con-ron. Use the toolchain the project pins:

```bash
elan toolchain install "$(cat lean-toolchain)"
```

You can do the same with no Synterra binary at all: write the files from the objects' JSON (PROTOCOL.md, section 2) and the configuration from section 4.

## 2. Reproduce an attestation (trust our binary, not the attester)

```bash
synterra import attestation.json    # or sync from a peer: synterra run --peer <url>
synterra reproduce <attestation-id>
```

`reproduce` re-judges the submission with *your* configuration and compares every gate:

| Exit code | Meaning |
|---|---|
| 0 | MATCH: same verdict, same status on every gate except the history-dependent `not_duplicate` |
| 1 | MISMATCH: the attester and you disagree. Keep both outputs and tell us (SECURITY.md) |
| 2 | INCOMPLETE: a gate could not run on your machine, so nothing was compared |

It never runs the command recorded in someone else's attestation; that would hand a stranger your shell. If the attestation's `params` differ from yours, it says so. To match them, set these in `synterra.toml`:

| Attestation field | What to match |
|---|---|
| `params.toolchain` | `elan toolchain install <it>`. It must be allowed by `checker.allowed_toolchains`. |
| `params.command` | `checker.command` |
| `params.trivial_tactics` | `checker.trivial_tactics` |
| `params.timeout_secs` | `checker.timeout_secs`. A timeout is "not judged", never a rejection. |
| `params.sandboxed` | Keep `checker.sandbox` non-empty. |
| `params.software` | The same release of this node, or your own implementation of PROTOCOL.md |

## 3. Check signatures and ids independently

Every object is plain JSON with an Ed25519 signature. PROTOCOL.md, section 1, is enough to verify one by hand. To check that your implementation, or ours, encodes the wire format correctly:

```bash
synterra vectors | diff - docs/test-vectors.json     # our binary agrees with the committed vectors
python3 scripts/check-vectors.py docs/test-vectors.json   # an independent Python check
```

`synterra verify` re-walks the local ledger: every hash link, every signature, every id.

## 4. Check the build

```bash
scripts/build-release.sh    # pinned toolchain (rust-toolchain.toml), --locked, path prefixes remapped
```

It prints the binary's SHA-256. The same commit, built on the same platform with the pinned toolchain, should give the same hash wherever it is built. When releases are published, their checksums will be published with them, so you can compare. No release has been published yet.

## 5. Run the tests

```bash
cargo test --locked                                   # unit tests: paths, names, signatures, cheats, vectors
BIN=target/release/synterra scripts/e2e-mock.sh      # two nodes over HTTP, with stand-ins for Lean
BIN=target/release/synterra scripts/e2e-lean.sh      # real Lean and bubblewrap, adversarial submissions
```

`e2e-lean.sh` is the one that matters. It judges:
- an honest solution;
- the target's names declared over weaker statements;
- a trivial lemma claim;
- a malformed claim;
- a `sorry`;
- a hostile solution that tries to read the node's private key while it builds.

It then has a second node with its own key reproduce the verdicts, and finally judges one submission by hand with `lake comparator`. The `mock-*` scripts are stand-ins for tests and check nothing.

## 6. Audit the dependencies

```bash
cargo install cargo-audit --locked && cargo audit     # RustSec advisory database
```

`Cargo.lock` is committed, and CI runs `cargo audit` on every change to `node/`.

## Found a disagreement?

A false acceptance is the most serious bug this software can have. See SECURITY.md.
