#!/usr/bin/env python3
"""Independent check of docs/test-vectors.json, using nothing from this repository's code.

Needs Python 3 and the `cryptography` package. Verifies that:
  - the public key is the ed25519 key of the published seed,
  - the signature verifies over the published signing bytes,
  - the signing bytes are "synterra/v1/<kind>/" + canonical body,
  - canonical JSON is sorted-key, separator-free JSON of the same value,
  - the id is SHA-256 of the canonical envelope.
"""
import hashlib, json, sys
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey
from cryptography.hazmat.primitives import serialization

def canonical(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)

doc = json.load(open(sys.argv[1] if len(sys.argv) > 1 else "docs/test-vectors.json"))
sk = Ed25519PrivateKey.from_private_bytes(bytes.fromhex(doc["secret_key_seed"]))
pk = sk.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw).hex()
assert pk == doc["public_key"], "public key does not match the seed"
for v in doc["vectors"]:
    body = json.loads(v["canonical_body"])
    assert canonical(body) == v["canonical_body"], "canonical body is not sorted, compact JSON"
    assert v["signing_bytes"] == f"synterra/v1/{v['kind']}/" + v["canonical_body"], "signing bytes layout"
    Ed25519PublicKey.from_public_bytes(bytes.fromhex(pk)).verify(bytes.fromhex(v["signature"]), v["signing_bytes"].encode())
    env = {"kind": v["kind"], "body": body, "signer": pk, "sig": v["signature"]}
    assert canonical(env) == v["canonical_envelope"], "canonical envelope"
    assert hashlib.sha256(v["canonical_envelope"].encode()).hexdigest() == v["id"], "id is not sha256 of the envelope"
print(f"ok: {len(doc['vectors'])} vector(s) verified independently")
