# Security policy

This node judges proofs written by strangers, so a security bug here can be a false verdict, not only a crash. Thank you for looking.

## Reporting

Report privately. Please don't open a public issue for a vulnerability.

- Email **security@counterbank.cc**, or
- use GitHub's private vulnerability reporting on this repository, if it is enabled.

Include what you ran (commit, toolchain, `synterra.toml`), what happened, and what you expected. For a verdict bug, include the objects (`synterra show <id>` for the target, the submission and the attestation), or the output of `synterra materialize`. We will reply, keep you informed while we fix it, and credit you in the changelog unless you would rather we didn't. There is no bug bounty programme yet.

## What counts

From most to least severe:

1. **False acceptance.** A submission signed `accepted` that does not prove the target's exact statements with permitted axioms only.
2. **Sandbox escape or key exposure.** Judged code that reads, writes or sends anything outside its work directory, or any route to the node key.
3. **False signed rejection.** A node signing `rejected` because of its own failure rather than the judge's verdict.
4. **Forgery.** A signature, id or ledger link that verifies when it should not.
5. **Path traversal, resource exhaustion, crashes** on input from peers.

Bugs in Lean's kernel, `lake comparator` or the bundled external kernels belong to the [Lean FRO](https://github.com/leanprover/lean4/security). Please tell us too, so nodes can pin away from an affected toolchain.

## Supported versions

Pre-release: only `main` is supported. The known gaps are listed in [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md); reports that extend one are welcome.

## Safe harbour

Good-faith research on nodes you run yourself is welcome, and we won't pursue it. Don't attack nodes run by other people, and don't publish an exploit before a fix is available.
