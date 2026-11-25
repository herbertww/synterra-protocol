# Threat model

## What we protect

1. **Verdict integrity.** A signed `accepted` must mean the submission proves the target's exact statements, with permitted axioms only, under Lean's kernel and every bundled external kernel. A signed `rejected` must mean the judge rejected it, and never that this node's own setup failed.
2. **The host and the node key.** Checking a proof must not hand the host, or the node's signing key, to the person who wrote the proof.
3. **The objects.** Nobody can alter a signed object, or pass off one object as another.

## Who we defend against

| Adversary | Wants | Can |
|---|---|---|
| Submitter | A false acceptance, a stolen key, a foothold on the host | Write any Lean, any claim, any bundle that passes the shape rules |
| Target nominator | Code execution on every checking node | Write the build configuration and pick the toolchain |
| Peer | To corrupt your store, waste your resources, mislead your verdicts | Serve any bytes, flood you with valid objects signed by throwaway keys |
| Attester | To make a false verdict look reproducible | Sign anything, including fake `params` |

## Defences

**Untrusted Lean runs only in a sandbox.**
- **The judge.** `lake comparator` builds the solution inside bubblewrap. Per Lean's own documentation, `/` is bound read-only and home directories are covered, so the code reads none of the invoking user's files. Only `.lake` is writable. The build, the export and the external kernels run with no network.
- **The triviality probes.** These run inside our own bubblewrap profile, which mounts only `/usr`, the target's pinned toolchain (read-only) and a fresh work directory. There is no home directory, no node key and no network, and the environment variables are cleared.
- **Tested.** `e2e-lean.sh` submits a proof that tries to read the node key at build time. It fails with "no such file or directory" and the key appears in no stored object.

**The judge is a toolchain the operator chose.**
- A target's `lean-toolchain` names the judge, and the judge runs outside the sandbox.
- The node therefore judges only with toolchains its policy allows (`checker.allowed_toolchains`, official `leanprover/lean4:v…` releases by default).
- The toolchain must already be installed, and the node calls that toolchain's own `lake` directly, never elan's proxy. Nothing is downloaded during a check.

**A broken checker never signs a rejection.** The node signs only on comparator exit code 0 (accepted) or 1 (rejected). Exit code 2, any other code, a crash, a timeout, a missing toolchain, or a probe that could not run all mean "not judged here": nothing is signed, and the submission is set aside locally.

**Hostile output never reaches a signature.** Gate details are fixed strings written by the node. Tool output goes only to the local log. Treat those logs as untrusted text.

**The solution cannot change the rules.**
- Solution bundles may hold only Lean modules under the solution module, and may not overwrite any target file.
- Lakefiles and toolchain pins cannot be submitted.
- The challenge is written before the solution, and each check gets a fresh directory that is deleted afterwards.

**Objects are content-addressed and signed.**
- Ids are SHA-256 of the canonical envelope.
- A peer that serves the wrong bytes for an id is ignored.
- Paths are restricted before anything touches the disk, and symlinks are never followed when reading a project.
- Every object is capped at 8 MiB.

**Verifiers never execute what an attestation says.** `reproduce` judges with the verifier's own configuration and only compares.

## Known gaps

These are open. Each one is a reason not to treat a single node's `accepted` as final.

1. **Claims are not yet bound to the lemma.** `not_trivial` and `not_duplicate` judge the statement a submitter claims, not the type of the lemma in the solution. A claim is rejected if it does not elaborate against the challenge, so a garbled claim cannot dodge the gate. A well-formed claim that differs from the real lemma, though, is still judged on its claim. The fix is to read lemma types from comparator's kernel export. Until then, passes from these gates are marked `advisory`.
2. **Duplicates are matched as text.** Two statements that differ only in variable names, or that are logically equivalent, are not caught, and Mathlib is not searched yet.
3. **Single-node verdicts.** There is no consensus. Acceptance should require several independent nodes, ideally on different machines, to agree.
4. **Spam and Sybils.** `POST /v1/objects` has no rate limit, and anyone can mint keys. The default listen address is loopback for this reason. A public node needs a reverse proxy with limits.
5. **Resource exhaustion.** A submission can use up to `timeout_secs` of CPU per check, per node.
6. **Dependency resolution has network.** Comparator allows the network while it fetches a target's git dependencies. Targets with dependencies trust those dependencies' hosts.
7. **The key is a plain file.** On Unix it is stored with mode 0600. On Windows its permissions are not narrowed. Use full-disk encryption.
8. **Remaining trust,** as in Lean's reference:
   - Lean's logic;
   - the comparator plumbing;
   - bubblewrap and the kernel's namespaces;
   - no bug shared by every kernel used.

   `--paranoid` shrinks the last one as far as Lean allows today.
