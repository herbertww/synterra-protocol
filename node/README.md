# Synterra node

The reference node for **Synterra proof mining**. Like a Bitcoin full node, it keeps a copy of everything, relays it to peers and verifies it itself. What it verifies is not a hash puzzle. It verifies proofs, using Lean's own judge.

> **Status: in build. Nothing has shipped.** This software issues no SYN. Scores are labelled *demo*: no issuance split, weight, band or payout schedule is decided, and nothing here is one.

## Don't trust us. Check.

| You want to | Run | Trust needed in our code |
|---|---|---|
| Judge a submission yourself | `synterra materialize <id> dir`, then `lake comparator --paranoid` in `dir` | none |
| Re-judge someone's attestation | `synterra reproduce <attestation>` | this binary |
| Check our wire format | `python3 scripts/check-vectors.py docs/test-vectors.json` | none |
| Check the build | `scripts/build-release.sh` (pinned toolchain, `--locked`) | none |

Details are in [docs/VERIFYING.md](docs/VERIFYING.md). The full protocol is in [docs/PROTOCOL.md](docs/PROTOCOL.md): enough to write an independent implementation.

## What a node does

| Role | Who | What |
|---|---|---|
| **Keep** | every node, any OS | Stores signed, content-addressed targets, submissions and attestations. |
| **Relay** | every node, any OS | Serves its inventory and pulls what it lacks from peers. Work is pulled, never pushed. |
| **Check** | Linux or WSL2, with Lean and bubblewrap | Judges each pending submission and signs an attestation. |

Low-compute participants can do all three. Checking is CPU work: a laptop running WSL2 judges exactly as a server does.

**One binary per platform.** The Windows build is `synterra.exe`; macOS and Linux get native builds. Every build keeps and relays. Checking needs Linux, because the sandbox does. When releases exist they will be signed and published with checksums. An unsigned .exe from a crypto project looks exactly like malware, and should.

## How a submission is judged

| Gate | Binding | Checks |
|---|---|---|
| `shape` | yes | Paths, sizes, module layout. A solution can hold only Lean modules and cannot overwrite any target file. |
| `no_sorry` | yes | No `sorry`, `admit`, `axiom`, `implemented_by`, `extern`. |
| `kernel` | yes | `lake comparator --paranoid`, Lean's own judge (in core since v4.35). It checks that the solution proves the same statements as the challenge, uses only permitted axioms, and is accepted by Lean's kernel and every external kernel bundled with Lean: leanchecker-paranoid, lean4lean, nanoda, con-leche and con-ron. |
| `not_trivial` | a fail is | Each claimed lemma must elaborate, and must not be closed unaided by `omega`, `decide`, `simp` or `grind`. A lemma a machine could find is not SYNful. |
| `not_duplicate` | a fail is | A lemma already accepted earns nothing. First accepted wins. |

A node signs a verdict only when the judge actually ran. A missing toolchain, a crash or a timeout means "not judged here", never "rejected".

## Security in one screen

Lean source is code, and a submission is written by a stranger who wants SYN. So:

- **Every run of untrusted Lean is sandboxed with bubblewrap.** The judged code sees no home directory, no node key and no network. `scripts/e2e-lean.sh` includes a proof that tries to steal the key; it gets "no such file or directory".
- **The judge is a toolchain you chose.** By default only official `leanprover/lean4` releases you installed yourself are used, called directly, and nothing is downloaded during a check.
- **Hostile output never reaches a signature.** Tool output stays in your local log.
- **Every object is re-validated on arrival,** and is content-addressed and Ed25519-signed.

The full model, including the known gaps, is in [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md). To report a vulnerability, see [SECURITY.md](SECURITY.md).

## Quick start

Requirements: Rust (the version in `rust-toolchain.toml` is fetched automatically by rustup). For checking: Linux or WSL2, [elan](https://github.com/leanprover/elan) and [bubblewrap](https://github.com/containers/bubblewrap).

```bash
cargo build --release --locked
alias synterra=target/release/synterra

synterra init                                   # data dir, node key, synterra.toml

# Nominate the example target (Gauss's sum) and submit a solution with one lemma.
cp -r examples/gauss /tmp/gauss && (cd /tmp/gauss && lake build)   # writes lake-manifest.json
elan toolchain install "$(cat examples/gauss/lean-toolchain)"
T=$(synterra nominate /tmp/gauss --name "Gauss sum" --theorem gauss --theorem gauss_succ)
S=$(synterra submit "$T" examples/solutions/good/Solution.lean \
      --lemma 'two_mul_sumTo::(n : Nat) : 2 * sumTo n = n * (n + 1)')

synterra check "$S"                             # judge now and sign
synterra run --peer http://<peer>:8590          # serve, pull, judge what is pending
```

Lemma claims are written in signature form, as they follow the lemma's name, and must be statable in the challenge's vocabulary.

## Configuration

`<datadir>/synterra.toml`, created by `init`:

| Key | Default | Meaning |
|---|---|---|
| `listen` | `127.0.0.1:8590` | Peer interface. Loopback by default; exposing it is a choice (THREAT-MODEL, known gaps). |
| `peers` | `[]` | Base URLs to pull from. |
| `sync_secs` | `30` | Pull and judge interval. |
| `checker.command` | `{toolchain}/bin/lake comparator --config={config} --paranoid` | The judge. |
| `checker.sandbox` | bubblewrap profile | Prefix for every other run of untrusted Lean. |
| `checker.trivial_tactics` | `omega`, `decide`, `(simp; done)`, `grind` | What makes a lemma trivial. |
| `checker.timeout_secs` | `900` | Per judge run. A timeout is "not judged". |
| `checker.allowed_toolchains` | `leanprover/lean4:v` | Toolchain pins this node will judge with. |

## Tests

```bash
cargo test --locked                                   # unit tests
BIN=target/release/synterra scripts/e2e-mock.sh      # two nodes over HTTP (stand-ins for Lean)
BIN=target/release/synterra scripts/e2e-lean.sh      # real Lean, adversarial submissions, independent verifier
```

CI runs all three on every change, plus `cargo audit` against the RustSec advisory database.

## Documentation

- [docs/PROTOCOL.md](docs/PROTOCOL.md): objects, encoding, signing, gates, network
- [docs/VERIFYING.md](docs/VERIFYING.md): every way to check our work
- [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md): what we defend, against whom, and the open gaps
- [SECURITY.md](SECURITY.md), [CONTRIBUTING.md](CONTRIBUTING.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), [CHANGELOG.md](CHANGELOG.md)

## Licence

Apache-2.0 (this directory), matching Mathlib, so mined Lean code can go upstream. Writing produced by the network is CC BY 4.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
