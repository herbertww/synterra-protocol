# Synterra proof-mining protocol, version 1

Status: **draft**. Nothing has shipped. This document is normative for Synterra Core, the reference node in this directory: where the code and this text disagree, that is a bug in one of them, and you should report it.

The aim is that anyone can write an independent implementation, judge the same submissions and reach the same verdicts, without reading our code. `docs/test-vectors.json` lets you check your encoding byte for byte, and `scripts/check-vectors.py` is an independent check written in Python.

## 1. Objects

Every object is an **envelope**:

```json
{"body": {…}, "kind": "target", "sig": "…", "signer": "…"}
```

| Field | Type | Meaning |
|---|---|---|
| `kind` | `"target"`, `"submission"` or `"attestation"` | What the body is. |
| `body` | object | The payload, schema per kind (section 3). |
| `signer` | 64 lowercase hex chars | Ed25519 public key (RFC 8032), 32 bytes. |
| `sig` | 128 lowercase hex chars | Ed25519 signature, 64 bytes, over the signing bytes. |

### 1.1 Canonical JSON

The canonical form of a JSON value is:

- object keys sorted by their UTF-8 bytes (equivalently, by code point), recursively;
- no whitespace outside strings;
- integers only: there are no fractional numbers in the protocol;
- strings escaped exactly as follows: `"` as `\"`, `\` as `\\`, U+0008 as `\b`, U+000C as `\f`, U+000A as `\n`, U+000D as `\r`, U+0009 as `\t`, every other code point below U+0020 as `\u00xx` with lowercase hex; everything else, including `/`, U+007F and all non-ASCII characters, written as raw UTF-8.

This is what `serde_json` produces for a sorted map, and what Python's `json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)` produces for the values this protocol uses.

### 1.2 Signing bytes

```
signing_bytes = "synterra/v1/" || kind || "/" || canonical(body)
```

The prefix is ASCII. The signature is pure Ed25519 over these bytes, with no pre-hashing.

### 1.3 Object id

```
id = lowercase_hex(SHA-256(canonical(envelope)))
```

An id addresses the whole envelope, signature included. Two different signers of the same body produce two different objects.

### 1.4 Limits

- Canonical envelope: at most 8 MiB.
- Bundles: 1 to 512 files, each at most 4 MiB.
- Every object is re-validated by every node that receives it: size, signature, schema and path rules. A node that serves the wrong bytes for an id is ignored.

## 2. Bundles

A bundle is `{"files": [{"path": …, "content": …}, …]}`: UTF-8 text files, sorted by `path` with no duplicates.

A path is 1 to 255 characters from `[A-Za-z0-9_./-]`, relative, with `/` separators, no empty segment, no `.` or `..` segment, and no segment that starts with `.`.

- **Project bundles** (targets) may hold `*.lean` files, `lakefile.toml`, `lakefile.lean`, `lean-toolchain` and `lake-manifest.json`.
- **Solution bundles** (submissions) may hold only Lean source modules: files ending in `.lean`, never one named `lakefile.lean`, and only the solution module's file (`Solution.lean` for module `Solution`) or files under its directory (`Solution/…`).

A Lean name is one or more dot-separated segments, each matching `[A-Za-z_][A-Za-z0-9_']*`, 200 characters at most in total.

## 3. Bodies

All bodies carry `v` (the protocol version, `1`) and `created` (Unix seconds, as the signer claims it; never trusted for ordering).

### 3.1 Target

| Field | Type | Rule |
|---|---|---|
| `name` | string | 1 to 120 characters. |
| `challenge_module` | Lean name | The module that states the target theorems. Must be in `project`. |
| `theorem_names` | Lean names, at least one | Every submission must prove each of these. |
| `permitted_axioms` | Lean names | Usually `propext`, `Quot.sound`, `Classical.choice`. |
| `project` | project bundle | Must contain `lean-toolchain` and `lake-manifest.json`. |
| `baseline_loc` | integer or null | Proof lines of the best known solution, for the demo score. |

A target is **trusted input**: whoever checks against it runs its build configuration. Nodes judge only targets they choose to follow (see THREAT-MODEL.md).

### 3.2 Submission

| Field | Type | Rule |
|---|---|---|
| `target` | id | The target this submission answers. |
| `solution_module` | Lean name | Not equal to the challenge module. |
| `solution` | solution bundle | No path may equal a path in the target's project. |
| `lemmas` | list of `{name, statement}`, at most 64 | Claimed lemmas. `name` is a Lean name. `statement` is 1 to 4000 characters in signature form: what follows the name in `theorem name …`, e.g. `(n : Nat) : 2 * sumTo n = n * (n + 1)`. A statement that does not begin with `(`, `{`, `[`, `⦃` or `:` is read as `: <statement>`. |
| `note` | string | At most 2000 characters. |

### 3.3 Attestation

| Field | Type | Meaning |
|---|---|---|
| `submission`, `target` | ids | What was judged. |
| `verdict` | `"accepted"` or `"rejected"` | Section 4. |
| `gates` | list of `{name, status, advisory, detail}` | `status` is `pass`, `fail` or `skipped`. |
| `score` | object or null | Present only when accepted: `{solution_loc, baseline_loc, loc_saved, label}`. The label always says demo. |
| `params` | object | Everything that can change a verdict: `{software, command, toolchain, trivial_tactics, timeout_secs, sandboxed}`. |

`detail` never contains output from the judged code: a hostile proof could print anything it can read. Tool output stays in the attesting node's local log.

## 4. Judging

Gates run in this order. The first failing binding gate ends the run.

| Gate | Fails when |
|---|---|
| `shape` | Any rule in sections 2 and 3.2 is broken. |
| `no_sorry` | The solution contains the token `sorry`, `admit`, `sorryAx`, `axiom`, `implemented_by` or `extern` anywhere except after `--` on a line. Block comments and docstrings count, so an honest solution should not mention these words there. |
| `kernel` | `lake comparator --config=<file> --paranoid`, run with the target's pinned toolchain, exits 1. |
| `not_trivial` | For some claim, the statement does not elaborate against the challenge module, or one of the trivial tactics closes it unaided. |
| `not_duplicate` | Some claim's statement, with runs of whitespace collapsed to one space, equals a statement this node accepted earlier, or another claim in the same submission. |

The comparator configuration is:

```json
{"challenge_module": "<target.challenge_module>", "solution_module": "<submission.solution_module>",
 "theorem_names": [<target.theorem_names>], "permitted_axioms": [<target.permitted_axioms>]}
```

The trivial tactics are the attestation's `params.trivial_tactics`. Each claim is probed twice inside a sandbox, after `lake build <challenge_module>`:

```lean
import <challenge_module>
example <signature> := by
  sorry
```

must elaborate (exit 0), and

```lean
import <challenge_module>
example <signature> := by
  first
  | <tactic 1>
  | …
```

must fail.

**Verdict.** `accepted` if no gate failed, otherwise `rejected`.

**Not judged.** A node does not sign anything for a submission it could not judge: the comparator exited with anything other than 0 or 1, timed out, or could not start; the toolchain is not installed or is outside the node's policy; or a triviality probe could not run. A broken checker must never look like a rejected proof.

**History.** `not_duplicate` and the demo score depend on what the attesting node accepted earlier, in its ledger order. Verifiers compare every other gate exactly and report this one.

## 5. Ledger

Each node keeps an append-only, hash-chained record of the objects it has stored:

```
entry = {height, prev, object, kind, ts, hash}
hash  = lowercase_hex(SHA-256("<height>|<prev>|<object>|<kind>|<ts>"))
```

`prev` of height 0 is 64 zeros. The ledger is local and is not consensus. Anchoring ledger heads and attestations on Solana is planned; until then, "first accepted" means first in a node's own ledger order.

## 6. Network

HTTP/1.1, JSON bodies. Default listen address: `127.0.0.1:8590`.

| Method and path | Returns |
|---|---|
| `GET /v1/info` | `{protocol, software, node, roles, height}` |
| `GET /v1/inventory` | Object ids, in the serving node's ledger order. |
| `GET /v1/objects/<id>` | The envelope. |
| `POST /v1/objects` | Body: an envelope. Validated, then stored. Returns `{id, added}`, or 422 with `{error}`. |

Sync is pull-only, like BitTorrent: a node fetches a peer's inventory, then each object it lacks, in the peer's order. It checks that each object hashes to the id it asked for, then validates and stores it.

## 7. Versioning

Any change to sections 1 to 4 that changes bytes, ids or verdicts bumps `v` and the signing-bytes prefix. A node rejects versions it does not implement.
