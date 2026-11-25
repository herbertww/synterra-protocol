# Contributing

Thank you. This node decides what counts as a proof, so the bar for changes is correctness first.

## Set up

- Rust: the version in `rust-toolchain.toml` (rustup picks it up automatically).
- For real checking (Linux or WSL2): Lean via [elan](https://github.com/leanprover/elan), then `elan toolchain install "$(cat examples/gauss/lean-toolchain)"`, and [bubblewrap](https://github.com/containers/bubblewrap) (`apt install bubblewrap`).

## Before you open a pull request

```bash
cargo fmt --check
cargo test --locked
cargo build --release --locked
BIN=target/release/synterra scripts/e2e-mock.sh
BIN=target/release/synterra scripts/e2e-lean.sh      # needs Lean and bubblewrap
python3 scripts/check-vectors.py docs/test-vectors.json
```

## Rules for changes

- **Anything that can change a verdict** needs an adversarial test: a submission that would have slipped through without your change. That covers `check.rs`, `bundle.rs`, `model.rs`, the default config and the protocol. The examples in `examples/solutions/` show the pattern.
- **Wire format changes** bump the protocol version, update [docs/PROTOCOL.md](docs/PROTOCOL.md), and regenerate the vectors: `synterra vectors > docs/test-vectors.json`.
- **New dependencies** need a reason in the pull request, and `cargo audit` must stay clean. Fewer dependencies is a security feature.
- **Never** put untrusted tool output into anything that gets signed, and never execute a command that came from an object.
- Lean examples follow Mathlib's style.

## Licence

This directory is licensed under Apache-2.0. Under section 5 of that licence, any contribution you submit is under the same terms (inbound = outbound), and no separate agreement is needed. Mined Lean code that goes upstream needs Apache-2.0 too, which is why Mathlib uses it.

## Conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
